use mudu::{sql_params, sql_stmt, XID, RS, ER::MuduError};
use mudu_macro::mudu_macro;
use chrono::Utc;
use uuid::Uuid;

// User management
#[mudu_macro]
pub fn create_user(xid: XID, phone: String) -> RS<String> {
    let user_id = Uuid::new_v4().to_string();
    command(
        xid,
        sql_stmt!("INSERT INTO users (user_id, phone) VALUES (?, ?)"),
        sql_params!(&[&user_id, &phone]),
    )?;
    Ok(user_id)
}

// Vote creation
#[mudu_macro]
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
        return Err(MuduError("End time must be in future".into()));
    }
    if vote_type != "single" && vote_type != "multiple" {
        return Err(MuduError("Vote type must be 'single' or 'multiple'".into()));
    }
    if vote_type == "single" && max_choices != 1 {
        return Err(MuduError("Single vote requires max_choices=1".into()));
    }
    if visibility_rule != "always" && visibility_rule != "after_end" {
        return Err(MuduError("Visibility rule must be 'always' or 'after_end'".into()));
    }

    let vote_id = Uuid::new_v4().to_string();
    command(
        xid,
        sql_stmt!(
            "INSERT INTO votes (vote_id, creator_id, topic, vote_type, max_choices, end_time, visibility_rule)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        ),
        sql_params!(&[&vote_id, &creator_id, &topic, &vote_type, &max_choices, &end_time, &visibility_rule]),
    )?;
    Ok(vote_id)
}

// Add option to vote
#[mudu_macro]
pub fn add_option(xid: XID, vote_id: String, option_text: String) -> RS<String> {
    let option_id = Uuid::new_v4().to_string();
    command(
        xid,
        sql_stmt!("INSERT INTO options (option_id, vote_id, option_text) VALUES (?, ?, ?)"),
        sql_params!(&[&option_id, &vote_id, &option_text]),
    )?;
    Ok(option_id)
}

// Submit vote
#[mudu_macro]
pub fn cast_vote(xid: XID, user_id: String, vote_id: String, option_ids: Vec<String>) -> RS<()> {
    // Check if vote is active
    let vote = query::<Vote>(
        xid,
        sql_stmt!("SELECT * FROM votes WHERE vote_id = ?"),
        sql_params!(&[&vote_id]),
    )?.next()?.ok_or_else(|| MuduError("Vote not found".into()))?;

    if Utc::now().timestamp() > vote.end_time {
        return Err(MuduError("Voting has ended".into()));
    }

    // Check user hasn't voted or has withdrawn previous vote
    let has_active_vote = query::<VoteAction>(
        xid,
        sql_stmt!("SELECT * FROM vote_actions WHERE user_id = ? AND vote_id = ? AND is_withdrawn = 0"),
        sql_params!(&[&user_id, &vote_id]),
    )?.next()?.is_some();

    if has_active_vote {
        return Err(MuduError("User already voted and hasn't withdrawn".into()));
    }

    // Validate choices
    if vote.vote_type == "single" && option_ids.len() != 1 {
        return Err(MuduError("Single vote requires exactly one option".into()));
    }
    if vote.vote_type == "multiple" && option_ids.len() > max_choices {
        return Err(MuduError("Exceeded max choices".into()));
    }

    // Create vote action
    let action_id = Uuid::new_v4().to_string();
    let action_time = Utc::now().timestamp();
    command(
        xid,
        sql_stmt!(
            "INSERT INTO vote_actions (action_id, user_id, vote_id, action_time)
             VALUES (?, ?, ?, ?)"
        ),
        sql_params!(&[&action_id, &user_id, &vote_id, &action_time]),
    )?;

    // Create vote choices
    for option_id in option_ids {
        let choice_id = Uuid::new_v4().to_string();
        command(
            xid,
            sql_stmt!(
                "INSERT INTO vote_choices (choice_id, action_id, option_id)
                 VALUES (?, ?, ?)"
            ),
            sql_params!(&[&choice_id, &action_id, &option_id]),
        )?;
    }

    Ok(())
}

// Withdraw vote
#[mudu_macro]
pub fn withdraw_vote(xid: XID, user_id: String, vote_id: String) -> RS<()> {
    let vote = query::<Vote>(
        xid,
        sql_stmt!("SELECT * FROM votes WHERE vote_id = ?"),
        sql_params!(&[&vote_id]),
    )?.next()?.ok_or_else(|| MuduError("Vote not found".into()))?;

    if Utc::now().timestamp() > vote.end_time {
        return Err(MuduError("Voting has ended, cannot withdraw".into()));
    }

    let active_action = query::<VoteAction>(
        xid,
        sql_stmt!("SELECT * FROM vote_actions WHERE user_id = ? AND vote_id = ? AND is_withdrawn = 0"),
        sql_params!(&[&user_id, &vote_id]),
    )?.next()?.ok_or_else(|| MuduError("No active vote to withdraw".into()))?;

    command(
        xid,
        sql_stmt!(
            "UPDATE vote_actions SET is_withdrawn = 1
             WHERE action_id = ?"
        ),
        sql_params!(&[&active_action.action_id]),
    )?;

    Ok(())
}

// Get vote results
#[mudu_macro]
pub fn get_vote_result(xid: XID, vote_id: String) -> RS<VoteResult> {
    let vote = query::<Vote>(
        xid,
        sql_stmt!("SELECT * FROM votes WHERE vote_id = ?"),
        sql_params!(&[&vote_id]),
    )?.next()?.ok_or_else(|| MuduError("Vote not found".into()))?;

    let now = Utc::now().timestamp();
    let vote_ended = now > vote.end_time;

    // Check visibility rules
    if vote.visibility_rule == "after_end" && !vote_ended {
        return Err(MuduError("Results only visible after vote ends".into()));
    }

    // Calculate results
    let mut options = query::<Option>(
        xid,
        sql_stmt!("SELECT * FROM options WHERE vote_id = ?"),
        sql_params!(&[&vote_id]),
    )?.collect::<Vec<_>>();

    let total_votes = query::<i64>(
        xid,
        sql_stmt!(
            "SELECT COUNT(*)
             FROM vote_actions
             WHERE vote_id = ? AND is_withdrawn = 0"
        ),
        sql_params!(&[&vote_id]),
    )?.next()?.unwrap_or(0);

    for option in &mut options {
        let count = query::<i64>(
            xid,
            sql_stmt!(
                "SELECT COUNT(*)
                 FROM vote_choices vc
                 JOIN vote_actions va ON vc.action_id = va.action_id
                 WHERE vc.option_id = ? AND va.vote_id = ? AND va.is_withdrawn = 0"
            ),
            sql_params!(&[&option.option_id, &vote_id]),
        )?.next()?.unwrap_or(0);
        option.count = count;
    }

    Ok(VoteResult {
        vote_id,
        topic: vote.topic,
        vote_ended,
        total_votes,
        options,
    })
}

// View voting history
#[mudu_macro]
pub fn get_voting_history(xid: XID, user_id: String) -> RS<Vec<VoteHistoryItem>> {
    let actions = query::<VoteAction>(
        xid,
        sql_stmt!(
            "SELECT va.*, v.topic
             FROM vote_actions va
             JOIN votes v ON va.vote_id = v.vote_id
             WHERE user_id = ?"
        ),
        sql_params!(&[&user_id]),
    )?.collect();

    let mut history = Vec::new();
    for action in actions {
        let vote_ended = Utc::now().timestamp() > action.end_time;
        history.push(VoteHistoryItem {
            vote_id: action.vote_id,
            topic: action.topic,
            action_time: action.action_time,
            is_withdrawn: action.is_withdrawn,
            vote_ended,
        });
    }

    Ok(history)
}