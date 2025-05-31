use gix::{bstr::{BString, ByteSlice}, date::time::format, objs::FindExt, revision::walk::Sorting, ObjectId, Repository};
use sqlite_loadable::{
    api,
    table::{ConstraintOperator, IndexInfo, VTab, VTabArguments, VTabCursor},
    BestIndexError, Result,
};
use sqlite_loadable::{prelude::*, Error};
use std::{any, mem, os::raw::c_int, path::Path};


static CREATE_SQL: &str = "CREATE TABLE x(commit_id,time,  author, message, repo hidden)";
enum Columns {
    Commit,
    Time,
    Author,
    Message,
    Repo
}

fn column(index: i32) -> Option<Columns> {
    match index {
          0 => Some(Columns::Commit),
          1 => Some(Columns::Time),
          2 => Some(Columns::Author),
          3 => Some(Columns::Message),
          4 => Some(Columns::Repo),
        _ => None,
    }
}
#[repr(C)]
pub struct GitLogTable {
    base: sqlite3_vtab,
}


impl<'vtab> VTab<'vtab> for GitLogTable {
    type Aux = ();
    type Cursor = GitLogCursor;

    fn connect(
        _db: *mut sqlite3,
        _aux: Option<&()>,
        _args: VTabArguments,
    ) -> Result<(String, GitLogTable)> {
        let base: sqlite3_vtab = unsafe { mem::zeroed() };
        let vtab = GitLogTable { base };
        // TODO db.config(VTabConfig::Innocuous)?;
        Ok((CREATE_SQL.to_owned(), vtab))
    }
    fn destroy(&self) -> Result<()> {
        Ok(())
    }


    fn best_index(&self, mut info: IndexInfo) -> core::result::Result<(), BestIndexError> {
      let mut has_repo = false;
      for mut constraint in info.constraints() {
        if constraint.op() == Some(ConstraintOperator::LIMIT) {
            constraint.set_argv_index(2);
            continue;
        }
          match column(constraint.column_idx()) {
              Some(Columns::Repo) => {
                  if constraint.usable() && constraint.op() == Some(ConstraintOperator::EQ) {
                      constraint.set_omit(true);
                      constraint.set_argv_index(1);
                      has_repo = true;
                  } else {
                      return Err(BestIndexError::Constraint);
                  }
              }

              _ => (),
          }
      }
      if !has_repo {
          return Err(BestIndexError::Error);
      }
      info.set_estimated_cost(100000.0);
      info.set_estimated_rows(100000);
      info.set_idxnum(2);

      Ok(())
  }

    fn open(&mut self) -> Result<GitLogCursor> {
        Ok(GitLogCursor::new())
    }
}

#[repr(C)]
pub struct GitLogCursor {
    base: sqlite3_vtab_cursor,
    rowid: i64,
    repo: Option<Repository>,
    commits: Vec<LogEntryInfo>,
}
impl GitLogCursor {
    fn new() -> GitLogCursor {
        let base: sqlite3_vtab_cursor = unsafe { mem::zeroed() };
        GitLogCursor {
            base,
            rowid: 0,
            repo: None,
            commits: Vec::new(),
        }
    }
}



impl VTabCursor for GitLogCursor{
    fn filter(
        &mut self,
        idx_num: c_int,
        _idx_str: Option<&str>,
        values: &[*mut sqlite3_value],
    ) -> Result<()> {
      let repo_path = api::value_text(&values[0])?;
      let limit = if values.len() >=2 {
        Some(api::value_int64(&values[1]) as usize)
      }else {
        None
      };
        let repo = gix::discover(Path::new(repo_path)).unwrap();
        let commit = repo.rev_parse_single("HEAD").unwrap().object().unwrap();
        let commit_id = commit.id;
        drop(commit);
        let commits: Vec<_> = repo.clone().rev_walk([commit_id])
            .sorting(Sorting::ByCommitTime(Default::default()))
            .all().unwrap()
            .filter(|info| {
                info.as_ref().map_or(true, |info| {
                    true
                })
            })
            .map(|info| -> LogEntryInfo {
                let info = info.unwrap();
                let commit = info.object().unwrap();
                let commit_ref = commit.decode().unwrap();
                LogEntryInfo {
                    commit_id: commit.id().to_hex().to_string(),
                    idx: commit.id().into(),
                    parents: info.parent_ids().map(|id| id.shorten_or_id().to_string()).collect(),
                    author: {
                        let mut buf = Vec::new();
                        commit_ref.author.actor().write_to(&mut buf).unwrap();
                        buf.into()
                    },
                    time: commit_ref.author.time().unwrap().format(format::ISO8601),
                    message: commit_ref.message.to_owned(),
                }
            })
            .take(limit.unwrap_or(usize::MAX))
            .collect();
        //let mut log_iter: Box<dyn Iterator<Item = anyhow::Result<LogEntryInfo>>> = Box::new();
        self.rowid = 0;
        self.commits = commits;
        self.repo = Some(repo);
        Ok(())
    }

    fn next(&mut self) -> Result<()> {
        self.rowid += 1;
        Ok(())
    }

    fn eof(&self) -> bool {
        self.commits.get(self.rowid as usize).is_none()
    }

    fn column(&self, context: *mut sqlite3_context, i: c_int) -> Result<()> {
      let commit = self.commits.get(self.rowid as usize)
            .ok_or_else(|| Error::new_message("No commit found for the current rowid"))?;
        match column(i) {
            Some(Columns::Commit) => api::result_text(context, &commit.commit_id)?,
            Some(Columns::Message) => api::result_text(context, commit.message.to_str_lossy())?,
            Some(Columns::Author) => api::result_text(context, commit.author.to_str_lossy())?,
            Some(Columns::Time) => api::result_text(context, &commit.time)?,
            Some(Columns::Repo) => api::result_pointer(context, c"repo".to_bytes(), self.repo.clone()),
            None => api::result_null(context),
        }
        Ok(())
    }

    fn rowid(&self) -> Result<i64> {
        Ok(self.rowid)
    }
}

struct LogEntryInfo {
    commit_id: String,
    idx: ObjectId,
    parents: Vec<String>,
    author: BString,
    time: String,
    message: BString,
}
