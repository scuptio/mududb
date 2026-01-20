use crate::rust::options::object::Options;
use crate::rust::vote_actions::object::VoteActions;
use crate::rust::vote_history_item::object::VoteHistoryItem;
use crate::rust::vote_result::object::VoteResult;
use crate::rust::votes::object::Votes;
use chrono::Utc;
use fallible_iterator::FallibleIterator;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::error::ec::EC::MuduError;
use mudu::m_error;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::{sql_params, sql_stmt};
use sys_interface::api::{mudu_command, mudu_query};
use uuid::Uuid;

// User management
/**mudu-proc**/
pub fn create_user(xid: XID, phone: String) -> RS<String> {
    let user_id = Uuid::new_v4().to_string();
    mudu_command(
        xid,
        sql_stmt!(&"INSERT INTO users (user_id, phone) VALUES (?, ?)"),
        sql_params!(&(user_id.clone(), phone)),
    )?;
    Ok(user_id)
}

// Vote creation
/**mudu-proc**/
pub fn create_vote(
    xid: XID,
    creator_id: String,
    topic: String,
    vote_type: String,
    max_choices: i64,
    end_time: i64,
    visibility_rule: String,
) -> RS<String> {
    // Validate input
    if end_time <= Utc::now().timestamp() {
        return Err(m_error!(MuduError, "End time must be in future".to_string()));
    }
    if vote_type != "single" && vote_type != "multiple" {
        return Err(m_error!(MuduError, "Vote type must be 'single' or 'multiple'".to_string()));
    }
    if vote_type == "single" && max_choices != 1 {
        return Err(m_error!(MuduError, "Single vote requires max_choices=1".to_string()));
    }
    if visibility_rule != "always" && visibility_rule != "after_end" {
        return Err(m_error!(MuduError, "Visibility rule must be 'always' or 'after_end'".to_string()));
    }

    let vote_id = Uuid::new_v4().to_string();
    mudu_command(
        xid,
        sql_stmt!(
            &"INSERT INTO votes (vote_id, creator_id, topic, vote_type, max_choices, end_time, visibility_rule)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        ),
        sql_params!(&(vote_id.clone(), creator_id, topic, vote_type, max_choices, end_time, visibility_rule)),
    )?;
    Ok(vote_id)
}

// Add option to vote
/**mudu-proc**/
pub fn add_option(xid: XID, vote_id: String, option_text: String) -> RS<String> {
    let option_id = Uuid::new_v4().to_string();
    mudu_command(
        xid,
        sql_stmt!(&"INSERT INTO options (option_id, vote_id, option_text) VALUES (?, ?, ?)"),
        sql_params!(&(option_id.clone(), vote_id, option_text)),
    )?;
    Ok(option_id)
}

// Submit vote
/**mudu-proc**/
pub fn cast_vote(xid: XID, user_id: String, vote_id: String, option_ids: Vec<String>) -> RS<()> {
    // Check if vote is active
    let vote = mudu_query::<Votes>(
        xid,
        sql_stmt!(&"SELECT * FROM votes WHERE vote_id = ?"),
        sql_params!(&(vote_id.clone(), )),
    )?.next()?.ok_or_else(|| m_error!(MuduError, "Vote not found".to_string()))?;

    if Utc::now().timestamp() > vote.get_end_time().unwrap() as i64 {
        return Err(m_error!(MuduError, "Voting has ended".to_string()));
    }

    // Check user hasn't voted or has withdrawn previous vote
    let mut rs:RecordSet<_> = mudu_query::<VoteActions>(
        xid,
        sql_stmt!(&"SELECT * FROM vote_actions WHERE user_id = ? AND vote_id = ? AND is_withdrawn = 0"),
        sql_params!(&(user_id.clone(), vote_id.clone())),
    )?;
    let has_active_vote = rs.next()?.is_some();

    if has_active_vote {
        return Err(m_error!(MuduError, "User already voted and hasn't withdrawn".to_string()));
    }

    // Validate choices
    if vote.get_vote_type().as_ref().unwrap() == "single" && option_ids.len() != 1 {
        return Err(m_error!(MuduError, "Single vote requires exactly one option".to_string()));
    }
    if vote.get_vote_type().as_ref().unwrap() == "multiple" && option_ids.len() > 3 {
        return Err(m_error!(MuduError, "Exceeded max choices".to_string()));
    }

    // Create vote action
    let action_id = Uuid::new_v4().to_string();
    let action_time = Utc::now().timestamp();
    mudu_command(
        xid,
        sql_stmt!(
            &"INSERT INTO vote_actions (action_id, user_id, vote_id, action_time)
             VALUES (?, ?, ?, ?)"
        ),
        sql_params!(&(action_id.clone(), user_id.clone(), vote_id, action_time)),
    )?;

    // Create vote choices
    for option_id in option_ids {
        let choice_id = Uuid::new_v4().to_string();
        mudu_command(
            xid,
            sql_stmt!(
                &"INSERT INTO vote_choices (choice_id, action_id, option_id)
                 VALUES (?, ?, ?)"
            ),
            sql_params!(&(choice_id, action_id.clone(), option_id)),
        )?;
    }

    Ok(())
}

// Withdraw vote
/**mudu-proc**/
pub fn withdraw_vote(xid: XID, user_id: String, vote_id: String) -> RS<()> {
    let vote = mudu_query::<Votes>(
        xid,
        sql_stmt!(&"SELECT * FROM votes WHERE vote_id = ?"),
        sql_params!(&(vote_id.clone(), )),
    )?.next()?.ok_or_else(|| m_error!(MuduError, "Vote not found".to_string()))?;

    if Utc::now().timestamp() > vote.get_end_time().unwrap() as i64 {
        return Err(m_error!(MuduError, "Voting has ended, cannot withdraw".to_string()));
    }

    let active_action = mudu_query::<VoteActions>(
        xid,
        sql_stmt!(&"SELECT * FROM vote_actions WHERE user_id = ? AND vote_id = ? AND is_withdrawn = 0"),
        sql_params!(&(user_id, vote_id)),
    )?.next()?.ok_or_else(|| m_error!(MuduError, "No active vote to withdraw".to_string()))?;

    let action_id = active_action.get_action_id().as_ref().unwrap().clone();
    mudu_command(
        xid,
        sql_stmt!(
            &"UPDATE vote_actions SET is_withdrawn = 1
             WHERE action_id = ?"
        ),
        sql_params!(&(action_id.clone(), )),
    )?;

    Ok(())
}

// Get vote results
/**mudu-proc**/
pub fn get_vote_result(xid: XID, vote_id: String) -> RS<VoteResult> {
    let vote = mudu_query::<Votes>(
        xid,
        sql_stmt!(&"SELECT * FROM votes WHERE vote_id = ?"),
        sql_params!(&(vote_id.clone(), )),
    )?.next()?.ok_or_else(|| m_error!(MuduError, "Vote not found".to_string()))?;

    let now = Utc::now().timestamp();
    let vote_ended = now > vote.get_end_time().unwrap() as i64;

    // Check visibility rules
    if vote.get_visibility_rule().as_ref().unwrap() == "after_end" && !vote_ended {
        return Err(m_error!(MuduError, "Results only visible after vote ends".to_string()));
    }

    // Calculate results
    let mut options = mudu_query::<Options>(
        xid,
        sql_stmt!(&"SELECT * FROM options WHERE vote_id = ?"),
        sql_params!(&(vote_id)),
    )?.collect::<Vec<_>>()?;

    let total_votes = mudu_query::<i64>(
        xid,
        sql_stmt!(
            &"SELECT COUNT(*)
             FROM vote_actions
             WHERE vote_id = ? AND is_withdrawn = 0"
        ),
        sql_params!(&(vote_id.clone(), )),
    )?.next()?.unwrap_or(0);

    for option in &mut options {
        let _count = mudu_query::<i64>(
            xid,
            sql_stmt!(
                &"SELECT COUNT(*)
                 FROM vote_choices vc
                 JOIN vote_actions va ON vc.action_id = va.action_id
                 WHERE vc.option_id = ? AND va.vote_id = ? AND va.is_withdrawn = 0"
            ),
            sql_params!(&(option.get_option_id().as_ref().unwrap().to_string(), vote_id.to_string())),
        )?.next()?.unwrap_or(0);
    }

    Ok(VoteResult::new(
        Some(vote_id),
        Some("topic".to_string()),
        Some(vote_ended as i32),
        Some(total_votes as i32),
        Some("todo".to_string()),
    ))
}

// View voting history
/**mudu-proc**/
pub fn get_voting_history(xid: XID, user_id: String) -> RS<Vec<VoteHistoryItem>> {
    let actions = mudu_query::<VoteActions>(
        xid,
        sql_stmt!(
            &"SELECT va.*, v.topic
             FROM vote_actions va
             JOIN votes v ON va.vote_id = v.vote_id
             WHERE user_id = ?"
        ),
        sql_params!(&(user_id.to_string(), )),
    )?.collect::<Vec<_>>()?;

    let mut history = Vec::new();
    for action in actions {
        let vote_ended = (Utc::now().timestamp() > action.get_action_time().unwrap() as i64) as i32;
        history.push(VoteHistoryItem::new(
            Some(action.get_vote_id().as_ref().unwrap().to_string()),
            Some("topic todo".to_string()),
            Some(action.get_action_time().unwrap()),
            Some(action.get_is_withdrawn().unwrap()),
            Some(vote_ended)
        ));
    }

    Ok(history)
}

