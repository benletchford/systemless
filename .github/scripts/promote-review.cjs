const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const CATALOGUE_WORKFLOW = 'www.yml';

function entryIds(files) {
  return [...new Set(files.filter(f => f.status !== 'removed')
    .map(f => /^www\/catalogue\/([a-z0-9]+(?:-[a-z0-9]+)*)\.md$/.exec(f.filename)?.[1])
    .filter(Boolean))].sort();
}
function validatePr(pr, repo, branch, sha) {
  if (pr.state !== 'open' || pr.base.ref !== branch || !pr.head.repo ||
      pr.head.repo.full_name !== repo || pr.head.ref === branch || (sha && pr.head.sha !== sha)) {
    throw new Error('PR must be open, target the default branch, and have an unchanged repository-owned head branch. Fork PRs require the manual promotion workflow.');
  }
}
function removalAllowed(file, ids) {
  return ids.some(id => file.startsWith(`catalogue/incoming/${id}/`)) &&
    !file.includes('\\') && !file.split('/').some(p => p === '..' || p === '.' || p === '');
}
async function approvedReview(github, context, number, reviewId, actor) {
  const {data: review} = await github.rest.pulls.getReview({...context.repo, pull_number: number, review_id: reviewId});
  if (review.state !== 'APPROVED' || review.user.login !== actor) throw new Error('Review is not an active approval by the triggering reviewer');
  const {data: permission} = await github.rest.repos.getCollaboratorPermissionLevel({...context.repo, username: actor});
  if (!['write', 'maintain', 'admin'].includes(permission.permission)) throw new Error('Only maintainers with write permission may promote assets');
  const reviews = await github.paginate(github.rest.pulls.listReviews, {...context.repo, pull_number: number});
  const latest = reviews.filter(r => r.user.login === actor).sort((a,b) => b.id-a.id)[0];
  if (!latest || latest.id !== reviewId || latest.state !== 'APPROVED') throw new Error('Approval has been superseded or dismissed');
  const {data: pr} = await github.rest.pulls.get({...context.repo, pull_number: number});
  validatePr(pr, `${context.repo.owner}/${context.repo.repo}`, context.payload.repository.default_branch, review.commit_id);
  return pr;
}
async function authorizedSubmission(github, context, state) {
  if (state.reviewId !== 0) {
    return approvedReview(github, context, state.number, state.reviewId, state.actor);
  }
  const {data: pr} = await github.rest.pulls.get({...context.repo, pull_number: state.number});
  if (state.actor !== context.repo.owner || pr.user.login !== context.repo.owner || pr.draft) {
    throw new Error('Owner promotion is no longer authorized');
  }
  validatePr(pr, `${context.repo.owner}/${context.repo.repo}`, context.payload.repository.default_branch, state.sha);
  return pr;
}
async function authorize({github, context, core}) {
  const run = context.payload.workflow_run;
  if (run.conclusion !== 'success') throw new Error('Expected a successful promotion authorization signal');
  const reviewMatch = /^Promotion review PR #(\d+) review #(\d+)$/.exec(run.display_title);
  const ownerMatch = /^Promotion owner PR #(\d+) commit ([0-9a-f]{40})$/.exec(run.display_title);
  let number;
  let reviewId;
  let actor = run.actor.login;
  let pr;
  if (run.event === 'pull_request_review' && reviewMatch) {
    number = Number(reviewMatch[1]);
    reviewId = Number(reviewMatch[2]);
    pr = await approvedReview(github, context, number, reviewId, actor);
  } else if (run.event === 'pull_request' && ownerMatch) {
    number = Number(ownerMatch[1]);
    const sha = ownerMatch[2];
    const {data: candidate} = await github.rest.pulls.get({...context.repo, pull_number: number});
    if (actor !== context.repo.owner || candidate.user.login !== context.repo.owner || candidate.draft) {
      throw new Error('Owner promotion requires a non-draft PR authored and triggered by the repository owner');
    }
    validatePr(candidate, `${context.repo.owner}/${context.repo.repo}`, context.payload.repository.default_branch, sha);
    pr = candidate;
    reviewId = 0;
  } else {
    throw new Error('Invalid promotion authorization signal');
  }
  core.setOutput('authorized', 'true');
  core.setOutput('number', number);
  const files = await github.paginate(github.rest.pulls.listFiles, {...context.repo, pull_number: pr.number});
  if (files.length !== pr.changed_files) throw new Error('Incomplete PR file list');
  const ids = entryIds(files);
  if (!ids.length) return;
  const state = {sha: pr.head.sha, branch: pr.head.ref, ids, number: pr.number, reviewId, actor};
  fs.writeFileSync('promotion-state.json', JSON.stringify(state));
  core.setOutput('sha', state.sha);
  core.setOutput('ids', JSON.stringify(ids));
}
async function commit({github, context, core}) {
  const state = JSON.parse(fs.readFileSync('promotion-state.json', 'utf8'));
  const pr = await authorizedSubmission(github, context, state);
  validatePr(pr, `${context.repo.owner}/${context.repo.repo}`, context.payload.repository.default_branch, state.sha);
  if (pr.head.ref !== state.branch) throw new Error('PR branch changed');
  const {data: base} = await github.rest.git.getCommit({...context.repo, commit_sha: state.sha});
  const {data: tree} = await github.rest.git.getTree({...context.repo, tree_sha: base.tree.sha, recursive: 'true'});
  if (tree.truncated) throw new Error('Incomplete source tree');
  const previous = new Map(tree.tree.map(item => [item.path, item]));
  const changes = [];
  for (const id of state.ids) {
    const filename = `www/catalogue/${id}.md`;
    const bytes = fs.readFileSync(path.join('submission', filename));
    const sha = crypto.createHash('sha1').update(`blob ${bytes.length}\0`).update(bytes).digest('hex');
    if (previous.get(filename)?.sha !== sha) {
      const {data: blob} = await github.rest.git.createBlob({...context.repo, content: bytes.toString('base64'), encoding: 'base64'});
      changes.push({path: filename, mode: '100644', type: 'blob', sha: blob.sha});
    }
    const result = JSON.parse(fs.readFileSync(`promotion-${id}.json`, 'utf8'));
    if (!result.applied) throw new Error('Promotion was not applied');
    for (const file of result.removed) {
      const repositoryPath = `www/${file}`;
      if (!removalAllowed(file, [id]) || previous.get(repositoryPath)?.type !== 'blob') throw new Error('Unexpected incoming deletion');
      changes.push({path: repositoryPath, mode: '100644', type: 'blob', sha: null});
    }
  }
  if (!changes.length) {
    core.setOutput('result', 'No pending assets; no commit was needed.');
  } else {
    const {data: updatedTree} = await github.rest.git.createTree({...context.repo, base_tree: base.tree.sha, tree: changes});
    const identity = {name: 'Ben Letchford', email: 'ben@letchford.cloud'};
    const {data: created} = await github.rest.git.createCommit({...context.repo,
      message: 'chore: promote catalogue assets', tree: updatedTree.sha, parents: [state.sha], author: identity, committer: identity});
    // A concurrent contributor push makes this non-fast-forward and is rejected.
    await github.rest.git.updateRef({...context.repo, ref: `heads/${state.branch}`, sha: created.sha, force: false});
    core.setOutput('result', `Promoted assets and committed rewrites as ${created.sha}.`);
  }
  await github.rest.actions.createWorkflowDispatch({...context.repo, workflow_id: CATALOGUE_WORKFLOW, ref: state.branch});
}
module.exports = {authorize, commit, entryIds, validatePr, removalAllowed, approvedReview, authorizedSubmission, CATALOGUE_WORKFLOW};
