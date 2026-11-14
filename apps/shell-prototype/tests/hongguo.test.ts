import {test} from 'node:test';
import assert from 'node:assert/strict';
import {loginPending,adjacent,message} from '../src/hongguo.ts';
test('login polling ends after confirmation, expiry or failure',()=>{
  assert.ok(loginPending('verifying'));assert.ok(loginPending('scanned'));
  for(const state of ['signed_in','expired','failed','idle'])assert.equal(loginPending(state),false);
});
test('episode navigation follows only the confirmed available list',()=>{
  assert.equal(adjacent([1,3,7],3,1),7);assert.equal(adjacent([1,3,7],1,-1),undefined);
  assert.equal(adjacent([1,3,7],2,1),undefined);assert.equal(message('login_required'),'请先登录红果账号');
});
