use crate::generated::options::object::Options;
use crate::generated::vote_actions::object::VoteActions;
use crate::generated::vote_history_item::object::VoteHistoryItem;
use crate::generated::vote_result::object::VoteResult;
use crate::generated::votes::object::Votes;
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
pub async fn create_user(xid: XID, phone: String) -> RS<String> {
    let user_id = Uuid::new_v4().to_string();
    mudu_command(
        xid,
        sql_stmt!(&"INSERT INTO users (user_id, phone) VALUES (?, ?)"),
        sql_params!(&(user_id.clone(), phone)),
    ).await?;
    Ok(user_id)
}

// Vote creation
/**mudu-proc**/
pub async fn create_vote(
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
    ).await?;
    Ok(vote_id)
}

// Add option to vote
/**mudu-proc**/
pub async fn add_option(xid: XID, vote_id: String, option_text: String) -> RS<String> {
    let option_id = Uuid::new_v4().to_string();
    mudu_command(
        xid,
        sql_stmt!(&"INSERT INTO options (option_id, vote_id, option_text) VALUES (?, ?, ?)"),
        sql_params!(&(option_id.clone(), vote_id, option_text)),
    ).await?;
    Ok(option_id)
}

// Submit vote
/**mudu-proc**/
pub async fn cast_vote(xid: XID, user_id: String, vote_id: String, option_ids: Vec<String>) -> RS<()> {
    // Check if vote is active
    let vote = mudu_query::<Votes>(
        xid,
        sql_stmt!(&"SELECT * FROM votes WHERE vote_id = ?"),
        sql_params!(&(vote_id.clone(), )),
    ).await?.next()?.ok_or_else(|| m_error!(MuduError, "Vote not found".to_string()))?;

    if Utc::now().timestamp() > vote.get_end_time().unwrap() as i64 {
        return Err(m_error!(MuduError, "Voting has ended".to_string()));
    }

    // Check user hasn't voted or has withdrawn previous vote
    let mut rs:RecordSet<_> = mudu_query::<VoteActions>(
        xid,
        sql_stmt!(&"SELECT * FROM vote_actions WHERE user_id = ? AND vote_id = ? AND is_withdrawn = 0"),
        sql_params!(&(user_id.clone(), vote_id.clone())),
    ).await?;
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
    ).await?;

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
        ).await?;
    }

    Ok(())
}

// Withdraw vote
/**mudu-proc**/
pub async fn withdraw_vote(xid: XID, user_id: String, vote_id: String) -> RS<()> {
    let vote = mudu_query::<Votes>(
        xid,
        sql_stmt!(&"SELECT * FROM votes WHERE vote_id = ?"),
        sql_params!(&(vote_id.clone(), )),
    ).await?.next()?.ok_or_else(|| m_error!(MuduError, "Vote not found".to_string()))?;

    if Utc::now().timestamp() > vote.get_end_time().unwrap() as i64 {
        return Err(m_error!(MuduError, "Voting has ended, cannot withdraw".to_string()));
    }

    let active_action = mudu_query::<VoteActions>(
        xid,
        sql_stmt!(&"SELECT * FROM vote_actions WHERE user_id = ? AND vote_id = ? AND is_withdrawn = 0"),
        sql_params!(&(user_id, vote_id)),
    ).await?.next()?.ok_or_else(|| m_error!(MuduError, "No active vote to withdraw".to_string()))?;

    let action_id = active_action.get_action_id().as_ref().unwrap().clone();
    mudu_command(
        xid,
        sql_stmt!(
            &"UPDATE vote_actions SET is_withdrawn = 1
             WHERE action_id = ?"
        ),
        sql_params!(&(action_id.clone(), )),
    ).await?;

    Ok(())
}

// Get vote results
/**mudu-proc**/
pub async fn get_vote_result(xid: XID, vote_id: String) -> RS<VoteResult> {
    let vote = mudu_query::<Votes>(
        xid,
        sql_stmt!(&"SELECT * FROM votes WHERE vote_id = ?"),
        sql_params!(&(vote_id.clone(), )),
    ).await?.next()?.ok_or_else(|| m_error!(MuduError, "Vote not found".to_string()))?;

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
    ).await?.collect::<Vec<_>>()?;

    let total_votes = mudu_query::<i64>(
        xid,
        sql_stmt!(
            &"SELECT COUNT(*)
             FROM vote_actions
             WHERE vote_id = ? AND is_withdrawn = 0"
        ),
        sql_params!(&(vote_id.clone(), )),
    ).await?.next()?.unwrap_or(0);

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
        ).await?.next()?.unwrap_or(0);
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
pub async fn get_voting_history(xid: XID, user_id: String) -> RS<Vec<VoteHistoryItem>> {
    let actions = mudu_query::<VoteActions>(
        xid,
        sql_stmt!(
            &"SELECT va.*, v.topic
             FROM vote_actions va
             JOIN votes v ON va.vote_id = v.vote_id
             WHERE user_id = ?"
        ),
        sql_params!(&(user_id.to_string(), )),
    ).await?.collect::<Vec<_>>()?;

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

async fn mp2_withdraw_vote(param: Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_withdraw_vote,
    ).await
}

pub async fn mudu_inner_p2_withdraw_vote(
    param: &::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_withdraw_vote().clone();
    let res = withdraw_vote(
        param.session_id(),
        
            ::mudu_type::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[0], "String")?,
        ::mudu_type::datum::value_to_typed::<
            String,
            _,
        >(&param.param_list()[1], "String")?,
    ).await;
    let tuple = res;
    Ok(
        ::mudu_contract::procedure::procedure_result::ProcedureResult::from(
            tuple,
            &return_desc,
        )?,
    )
}

pub fn mudu_argv_desc_withdraw_vote() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            <(
                String,
                String,
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum
            >::tuple_desc_static(
                &{
                    let _vec: Vec<String> = <[_]>::into_vec(
                            std::boxed::Box::new([
                                "user_id",
                                "vote_id",
                            

                            ]),
                        )
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    _vec
                },
            )
        }
    )
}

pub fn mudu_result_desc_withdraw_vote() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            <() as ::mudu_contract::tuple::tuple_datum::TupleDatum>::tuple_desc_static(
                &[],
            )
        }
    )
}

pub fn mudu_proc_desc_withdraw_vote() -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "module".to_string(),
                "withdraw_vote".to_string(),
                mudu_argv_desc_withdraw_vote().clone(),
                mudu_result_desc_withdraw_vote().clone(),
                false
            )
        })
}

mod mod_withdraw_vote {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-withdraw-vote;
            world mudu-app-mp2-withdraw-vote {
                export mp2-withdraw-vote: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestWithdrawVote {}

    impl Guest for GuestWithdrawVote {
        async fn mp2_withdraw_vote(param: Vec<u8>) -> Vec<u8> {
            super::mp2_withdraw_vote(param).await
        }
    }

    export!(GuestWithdrawVote);
}
async fn mp2_get_vote_result(param: Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_get_vote_result,
    ).await
}

pub async fn mudu_inner_p2_get_vote_result(
    param: &::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_get_vote_result().clone();
    let res = get_vote_result(
        param.session_id(),
        ::mudu_type::datum::value_to_typed::<
            String,
            _,
        >(&param.param_list()[0], "String")?,
    ).await;
    let tuple = res;
    Ok(
        ::mudu_contract::procedure::procedure_result::ProcedureResult::from(
            tuple,
            &return_desc,
        )?,
    )
}

pub fn mudu_argv_desc_get_vote_result() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            <(
                String,
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum
            >::tuple_desc_static(
                &{
                    let _vec: Vec<String> = <[_]>::into_vec(
                        std::boxed::Box::new([
                            "vote_id",
                        ]),
                    )
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    _vec
                },
            )
        }
    )
}

pub fn mudu_result_desc_get_vote_result() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            <(
                VoteResult,
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum>::tuple_desc_static(
                &[],
            )
        }
    )
}

pub fn mudu_proc_desc_get_vote_result() -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "module".to_string(),
                "get_vote_result".to_string(),
                mudu_argv_desc_get_vote_result().clone(),
                mudu_result_desc_get_vote_result().clone(),
                false
            )
        })
}

mod mod_get_vote_result {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-get-vote-result;
            world mudu-app-mp2-get-vote-result {
                export mp2-get-vote-result: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestGetVoteResult {}

    impl Guest for GuestGetVoteResult {
        async fn mp2_get_vote_result(param: Vec<u8>) -> Vec<u8> {
            super::mp2_get_vote_result(param).await
        }
    }

    export!(GuestGetVoteResult);
}
async fn mp2_cast_vote(param:Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_cast_vote,
    ).await
}

pub async fn mudu_inner_p2_cast_vote(
    param: &::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_cast_vote().clone();
    let res = cast_vote(
        param.session_id(),
        
            ::mudu_type::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[0], "String")?,
        
            ::mudu_type::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[1], "String")?,
        
            ::mudu_type::datum::value_to_typed::<
                Vec<String, >,
                _,
            >(&param.param_list()[2], "Vec<String, >")?,
        
    ).await;
    let tuple = res;
    Ok(
        ::mudu_contract::procedure::procedure_result::ProcedureResult::from(
            tuple,
            &return_desc,
        )?,
    )
}

pub fn mudu_argv_desc_cast_vote()  -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            <(
                
                    String,
                
                    String,
                
                    Vec<String, >,
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum
            >::tuple_desc_static(
                &{
                    let _vec: Vec<String> = <[_]>::into_vec(
                            std::boxed::Box::new([
                            
                                "user_id",
                            
                                "vote_id",
                            
                                "option_ids",
                            

                            ]),
                        )
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    _vec
                },
            )
        }
    )
}

pub fn mudu_result_desc_cast_vote() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            <(
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum>::tuple_desc_static(
                &[],
            )
        }
    )
}

pub fn mudu_proc_desc_cast_vote()  -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "module".to_string(),
                "cast_vote".to_string(),
                mudu_argv_desc_cast_vote().clone(),
                mudu_result_desc_cast_vote().clone(),
                false
            )
        })
}

mod mod_cast_vote {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-cast-vote;
            world mudu-app-mp2-cast-vote {
                export mp2-cast-vote: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestCastVote {}

    impl Guest for GuestCastVote {
        async fn mp2_cast_vote(param:Vec<u8>) -> Vec<u8> {
            super::mp2_cast_vote(param).await
        }
    }

    export!(GuestCastVote);
}
async fn mp2_create_vote(param: Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_create_vote,
    ).await
}

pub async fn mudu_inner_p2_create_vote(
    param: &::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_create_vote().clone();
    let res = create_vote(
        param.session_id(),
        
            ::mudu_type::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[0], "String")?,
        
            ::mudu_type::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[1], "String")?,
        ::mudu_type::datum::value_to_typed::<
            String,
            _,
        >(&param.param_list()[2], "String")?,
        ::mudu_type::datum::value_to_typed::<
            i64,
            _,
        >(&param.param_list()[3], "i64")?,
        ::mudu_type::datum::value_to_typed::<
            i64,
            _,
        >(&param.param_list()[4], "i64")?,
        ::mudu_type::datum::value_to_typed::<
            String,
            _,
        >(&param.param_list()[5], "String")?,
    ).await;
    let tuple = res;
    Ok(
        ::mudu_contract::procedure::procedure_result::ProcedureResult::from(
            tuple,
            &return_desc,
        )?,
    )
}

pub fn mudu_argv_desc_create_vote() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            <(
                
                    String,
                
                    String,
                    String,
                    i64,
                    i64,
                    String,
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum
            >::tuple_desc_static(
                &{
                    let _vec: Vec<String> = <[_]>::into_vec(
                            std::boxed::Box::new([
                                "creator_id",
                                "topic",
                                "vote_type",
                                "max_choices",
                                "end_time",
                                "visibility_rule",
                            

                            ]),
                        )
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    _vec
                },
            )
        }
    )
}

pub fn mudu_result_desc_create_vote() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            <(
                String,
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum>::tuple_desc_static(
                &[],
            )
        }
    )
}

pub fn mudu_proc_desc_create_vote() -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "module".to_string(),
                "create_vote".to_string(),
                mudu_argv_desc_create_vote().clone(),
                mudu_result_desc_create_vote().clone(),
                false
            )
        })
}

mod mod_create_vote {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-create-vote;
            world mudu-app-mp2-create-vote {
                export mp2-create-vote: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestCreateVote {}

    impl Guest for GuestCreateVote {
        async fn mp2_create_vote(param: Vec<u8>) -> Vec<u8> {
            super::mp2_create_vote(param).await
        }
    }

    export!(GuestCreateVote);
}
async fn mp2_add_option(param: Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_add_option,
    ).await
}

pub async fn mudu_inner_p2_add_option(
    param: &::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_add_option().clone();
    let res = add_option(
        param.session_id(),
        
            ::mudu_type::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[0], "String")?,
        ::mudu_type::datum::value_to_typed::<
            String,
            _,
        >(&param.param_list()[1], "String")?,
    ).await;
    let tuple = res;
    Ok(
        ::mudu_contract::procedure::procedure_result::ProcedureResult::from(
            tuple,
            &return_desc,
        )?,
    )
}

pub fn mudu_argv_desc_add_option() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            <(
                
                    String,
                    String,
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum
            >::tuple_desc_static(
                &{
                    let _vec: Vec<String> = <[_]>::into_vec(
                            std::boxed::Box::new([
                            
                                "vote_id",
                                "option_text",
                            ]),
                        )
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    _vec
                },
            )
        }
    )
}

pub fn mudu_result_desc_add_option() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            <(
                String,
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum>::tuple_desc_static(
                &[],
            )
        }
    )
}

pub fn mudu_proc_desc_add_option() -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "module".to_string(),
                "add_option".to_string(),
                mudu_argv_desc_add_option().clone(),
                mudu_result_desc_add_option().clone(),
                false
            )
        })
}

mod mod_add_option {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-add-option;
            world mudu-app-mp2-add-option {
                export mp2-add-option: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestAddOption {}

    impl Guest for GuestAddOption {
        async fn mp2_add_option(param: Vec<u8>) -> Vec<u8> {
            super::mp2_add_option(param).await
        }
    }

    export!(GuestAddOption);
}
async fn mp2_get_voting_history(param:Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_get_voting_history,
    ).await
}

pub async fn mudu_inner_p2_get_voting_history(
    param: &::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_get_voting_history().clone();
    let res = get_voting_history(
        param.session_id(),
        
            ::mudu_type::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[0], "String")?,
        
    ).await;
    let tuple = res;
    Ok(
        ::mudu_contract::procedure::procedure_result::ProcedureResult::from(
            tuple,
            &return_desc,
        )?,
    )
}

pub fn mudu_argv_desc_get_voting_history()  -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            <(
                
                    String,
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum
            >::tuple_desc_static(
                &{
                    let _vec: Vec<String> = <[_]>::into_vec(
                            std::boxed::Box::new([
                            
                                "user_id",
                            

                            ]),
                        )
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    _vec
                },
            )
        }
    )
}

pub fn mudu_result_desc_get_voting_history() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            <(
                
                    Vec<VoteHistoryItem, >,
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum>::tuple_desc_static(
                &[],
            )
        }
    )
}

pub fn mudu_proc_desc_get_voting_history()  -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "module".to_string(),
                "get_voting_history".to_string(),
                mudu_argv_desc_get_voting_history().clone(),
                mudu_result_desc_get_voting_history().clone(),
                false
            )
        })
}

mod mod_get_voting_history {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-get-voting-history;
            world mudu-app-mp2-get-voting-history {
                export mp2-get-voting-history: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestGetVotingHistory {}

    impl Guest for GuestGetVotingHistory {
        async fn mp2_get_voting_history(param:Vec<u8>) -> Vec<u8> {
            super::mp2_get_voting_history(param).await
        }
    }

    export!(GuestGetVotingHistory);
}
async fn mp2_create_user(param: Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_create_user,
    ).await
}

pub async fn mudu_inner_p2_create_user(
    param: &::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_create_user().clone();
    let res = create_user(
        param.session_id(),
        
            ::mudu_type::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[0], "String")?,
        
    ).await;
    let tuple = res;
    Ok(
        ::mudu_contract::procedure::procedure_result::ProcedureResult::from(
            tuple,
            &return_desc,
        )?,
    )
}

pub fn mudu_argv_desc_create_user() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            <(
                
                    String,
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum
            >::tuple_desc_static(
                &{
                    let _vec: Vec<String> = <[_]>::into_vec(
                            std::boxed::Box::new([
                                "phone",
                            

                            ]),
                        )
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    _vec
                },
            )
        }
    )
}

pub fn mudu_result_desc_create_user() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            <(
                
                    String,
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum>::tuple_desc_static(
                &[],
            )
        }
    )
}

pub fn mudu_proc_desc_create_user() -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "module".to_string(),
                "create_user".to_string(),
                mudu_argv_desc_create_user().clone(),
                mudu_result_desc_create_user().clone(),
                false
            )
        })
}

mod mod_create_user {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-create-user;
            world mudu-app-mp2-create-user {
                export mp2-create-user: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestCreateUser {}

    impl Guest for GuestCreateUser {
        async fn mp2_create_user(param: Vec<u8>) -> Vec<u8> {
            super::mp2_create_user(param).await
        }
    }

    export!(GuestCreateUser);
}