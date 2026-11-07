const {test} = require('node:test');
const assert = require('node:assert/strict');
const {entryIds, validatePr, removalAllowed, authorizedSubmission, CATALOGUE_WORKFLOW} = require('./promote-review.cjs');
test('promotion explicitly dispatches the website and catalogue workflow', () => {
  assert.equal(CATALOGUE_WORKFLOW, 'www.yml');
});
test('only actual changed entry paths become CLI arguments', () => {
  assert.deepEqual(entryIds([{filename:'www/catalogue/marathon.md'}, {filename:'www/catalogue/ev.md',status:'removed'},
    {filename:'www/catalogue/../../bad.md'}, {filename:'www/catalogue/a;echo.md'}, {filename:'.github/workflows/ci.yml'},
    {filename:'www/catalogue/marathon.md'}]), ['marathon']);
});
test('closed, forked, retargeted, default-head and changed-head PRs are rejected', () => {
  const pr = {state:'open',base:{ref:'master'},head:{ref:'dev/test',sha:'abc',repo:{full_name:'owner/repo'}}};
  validatePr(pr,'owner/repo','master','abc');
  for (const altered of [{...pr,state:'closed'}, {...pr,base:{ref:'other'}},
    {...pr,head:{...pr.head,sha:'new'}}, {...pr,head:{...pr.head,ref:'master'}},
    {...pr,head:{...pr.head,repo:{full_name:'fork/repo'}}}]) {
    assert.throws(() => validatePr(altered,'owner/repo','master','abc'));
  }
});
test('cleanup cannot escape the selected entry incoming directory', () => {
  assert.equal(removalAllowed('catalogue/incoming/marathon/shot.png',['marathon']),true);
  for (const file of ['catalogue/marathon.md','catalogue/incoming/other/shot.png','catalogue/incoming/marathon/../other/a','catalogue/incoming/marathon//a','catalogue/incoming/marathon/a\\b']) {
    assert.equal(removalAllowed(file,['marathon']),false);
  }
});
test('non-approved and mismatched-author reviews are rejected', async () => {
  const {approvedReview} = require('./promote-review.cjs');
  for (const review of [{state:'COMMENTED',user:{login:'maintainer'}}, {state:'APPROVED',user:{login:'other'}}]) {
    await assert.rejects(approvedReview({rest:{pulls:{getReview:async()=>({data:review})}}}, {repo:{}}, 1, 2, 'maintainer'), /active approval/);
  }
});
test('unprivileged approvals cannot trigger promotion', async () => {
  const {approvedReview} = require('./promote-review.cjs');
  await assert.rejects(approvedReview({rest:{pulls:{getReview:async()=>({data:{state:'APPROVED',user:{login:'contributor'}}})},
    repos:{getCollaboratorPermissionLevel:async()=>({data:{permission:'read'}})}}}, {repo:{}}, 1, 2, 'contributor'), /Only maintainers/);
});
test('superseded approvals and changed reviewed commits cannot promote', async () => {
  const {approvedReview} = require('./promote-review.cjs');
  const review = {id:2,state:'APPROVED',commit_id:'reviewed',user:{login:'maintainer'}};
  const pr = {state:'open',base:{ref:'master'},head:{ref:'dev/test',sha:'new',repo:{full_name:'owner/repo'}}};
  const github = {rest:{pulls:{getReview:async()=>({data:review}),get:async()=>({data:pr})},
    repos:{getCollaboratorPermissionLevel:async()=>({data:{permission:'write'}})}},paginate:async()=>[review]};
  const context = {repo:{owner:'owner',repo:'repo'},payload:{repository:{default_branch:'master'}}};
  await assert.rejects(approvedReview(github,context,1,2,'maintainer'),/unchanged/);
  pr.head.sha='reviewed';
  assert.equal(await approvedReview(github,context,1,2,'maintainer'),pr);
  github.paginate=async()=>[review,{...review,id:3,state:'CHANGES_REQUESTED'}];
  await assert.rejects(approvedReview(github,context,1,2,'maintainer'),/superseded/);
});
test('repository owner submissions allow drafts but require the owner and an unchanged head', async () => {
  const pr = {state:'open',draft:true,user:{login:'owner'},base:{ref:'master'},head:{ref:'dev/test',sha:'reviewed',repo:{full_name:'owner/repo'}}};
  const github = {rest:{pulls:{get:async()=>({data:pr})}}};
  const context = {repo:{owner:'owner',repo:'repo'},payload:{repository:{default_branch:'master'}}};
  const state = {number:1,reviewId:0,actor:'owner',sha:'reviewed'};
  assert.equal(await authorizedSubmission(github,context,state),pr);
  await assert.rejects(authorizedSubmission(github,context,{...state,actor:'other'}),/no longer authorized/);
  pr.user.login='other';
  await assert.rejects(authorizedSubmission(github,context,state),/no longer authorized/);
  pr.user.login='owner';
  pr.head.sha='changed';
  await assert.rejects(authorizedSubmission(github,context,state),/unchanged/);
});

// Run commit fixtures in separate processes because promotion reads relative paths.
function runCommitFixture(changed, rejectUpdate = false) {
  const fs = require('node:fs');
  const os = require('node:os');
  const path = require('node:path');
  const {spawnSync} = require('node:child_process');
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'promotion-commit-'));
  try {
    const bytes = Buffer.from('promoted entry\n');
    const sha = require('node:crypto').createHash('sha1')
      .update(`blob ${bytes.length}\0`).update(bytes).digest('hex');
    fs.mkdirSync(path.join(directory, 'submission/www/catalogue'), {recursive: true});
    fs.writeFileSync(path.join(directory, 'submission/www/catalogue/game.md'), bytes);
    fs.writeFileSync(path.join(directory, 'promotion-state.json'), JSON.stringify({
      sha:'original', branch:'dev/test', ids:['game'], number:1, reviewId:0, actor:'owner'
    }));
    fs.writeFileSync(path.join(directory, 'promotion-game.json'), JSON.stringify({applied:true, removed:[]}));
    const script = `
      const {commit} = require(${JSON.stringify(require.resolve('./promote-review.cjs'))});
      const events = [];
      const record = (name, data) => async args => {
        events.push({name, args});
        if (name === 'updateRef' && ${rejectUpdate}) throw new Error('concurrent push');
        return {data};
      };
      const github = {rest:{
        pulls:{get:async()=>({data:{state:'open', user:{login:'owner'}, base:{ref:'master'},
          head:{ref:'dev/test', sha:'original', repo:{full_name:'owner/repo'}}}})},
        git:{
          getCommit:async()=>({data:{tree:{sha:'base-tree'}}}),
          getTree:async()=>({data:{truncated:false, tree:[
            {path:'www/catalogue/game.md', type:'blob', sha:${JSON.stringify(changed ? 'old-blob' : sha)}}
          ]}}),
          createBlob:record('createBlob', {sha:'new-blob'}),
          createTree:record('createTree', {sha:'new-tree'}),
          createCommit:record('createCommit', {sha:'promoted'}),
          updateRef:record('updateRef', {})
        },
        actions:{createWorkflowDispatch:record('dispatch', {})}
      }};
      const context = {repo:{owner:'owner', repo:'repo'}, payload:{repository:{default_branch:'master'}}};
      commit({github, context, core:{setOutput(){}}})
        .then(()=>console.log(JSON.stringify({events})))
        .catch(error=>console.log(JSON.stringify({events, error:error.message})));
    `;
    const result = spawnSync(process.execPath, ['-e', script], {cwd:directory, encoding:'utf8'});
    assert.equal(result.status, 0, result.stderr);
    return JSON.parse(result.stdout);
  } finally {
    fs.rmSync(directory, {recursive:true, force:true});
  }
}
test('unchanged promotion neither writes Git objects nor dispatches duplicate CI', () => {
  assert.deepEqual(runCommitFixture(false), {events:[]});
});
test('changed promotion dispatches fresh CI only after updating the promoted branch', () => {
  const result = runCommitFixture(true);
  assert.equal(result.error, undefined);
  assert.deepEqual(result.events.map(event=>event.name),
    ['createBlob', 'createTree', 'createCommit', 'updateRef', 'dispatch']);
  assert.deepEqual(result.events[3].args,
    {owner:'owner', repo:'repo', ref:'heads/dev/test', sha:'promoted', force:false});
  assert.deepEqual(result.events[4].args,
    {owner:'owner', repo:'repo', workflow_id:'www.yml', ref:'dev/test'});
});
test('rejected concurrent branch update does not dispatch CI', () => {
  const result = runCommitFixture(true, true);
  assert.equal(result.error, 'concurrent push');
  assert.deepEqual(result.events.map(event=>event.name),
    ['createBlob', 'createTree', 'createCommit', 'updateRef']);
});
