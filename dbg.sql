.load dist/debug/git0
.mode qbox


select 
  *,
  git_at(repo, commit_id, 'README.md')
from git_log('/Users/alex/work/simonw/datasette-alerts/examples/florida-power/scrape-florida-outages')
limit 10;