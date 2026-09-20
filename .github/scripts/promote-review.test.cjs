const {test} = require('node:test');
const assert = require('node:assert/strict');
const {entryIds, validatePr, removalAllowed, authorize} = require('./promote-review.cjs');
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
