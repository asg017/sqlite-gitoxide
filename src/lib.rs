mod log;
use std::str::FromStr;

use gix::{ObjectId, Repository};
use log::GitLogTable;
use sqlite_loadable::{
    api, define_scalar_function, define_table_function, prelude::*, Error, Result,
};

pub fn git_version(context: *mut sqlite3_context, _values: &[*mut sqlite3_value]) -> Result<()> {
    api::result_text(context, format!("v{}", env!("CARGO_PKG_VERSION")))?;
    Ok(())
}

pub fn git_debug(context: *mut sqlite3_context, _values: &[*mut sqlite3_value]) -> Result<()> {
    api::result_text(
        context,
        format!(
            "Version: v{}
Source: {}
",
            env!("CARGO_PKG_VERSION"),
            env!("GIT_HASH")
        ),
    )?;
    Ok(())
}

fn git_at(context: *mut sqlite3_context, values: &[*mut sqlite3_value]) -> Result<()> {
    let repo: &Repository = unsafe {
        api::value_pointer::<Repository>(&values[0], c"repo".to_bytes())
            .ok_or_else(|| Error::new_message("1st argument is not a repository pointer"))?
            .as_ref()
            .ok_or_else(|| Error::new_message("1st argument is not a repository pointer"))?
    };
    let commit_id = ObjectId::from_str(api::value_text(&values[1])?).map_err(|e| Error::new_message(format!("Invalid commit ID: {}", e)))?;
    let commit = repo
        .find_commit(commit_id)
        .map_err(|e| Error::new_message(format!("Failed to find commit: {}", e)))?;
    let tree = commit.tree()
        .map_err(|e| Error::new_message(format!("Failed to get tree from commit: {}", e)))?;
    let entry  = match tree.lookup_entry_by_path(api::value_text(&values[2])?) {
        Ok(Some(entry)) => entry,
        _ => {
          return Ok(());
        }
    };
    let data = &entry.object().map_err(|e| Error::new_message(format!("entry is not an object: {e}")))?.data;
    match std::str::from_utf8(data) {
        Ok(s) => api::result_text(context, s)?,
        Err(_) => api::result_blob(context, data),
    }
    Ok(())
}

#[sqlite_entrypoint]
pub fn sqlite3_git_init(db: *mut sqlite3) -> Result<()> {
    define_scalar_function(
        db,
        "git_version",
        0,
        git_version,
        FunctionFlags::UTF8 | FunctionFlags::DETERMINISTIC,
    )?;
    define_scalar_function(
        db,
        "git_debug",
        0,
        git_debug,
        FunctionFlags::UTF8 | FunctionFlags::DETERMINISTIC,
    )?;
    define_scalar_function(db, "git_at", 3, git_at, FunctionFlags::UTF8)?;

    define_table_function::<GitLogTable>(db, "git_log", None)?;
    Ok(())
}
