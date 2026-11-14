import {test} from 'node:test';
import assert from 'node:assert/strict';
import {loginPending,adjacent,message,qualityLabels} from '../src/hongguo.ts';
test('numeric service qualities become canonical playback request strings',()=>{
  assert.deepEqual(qualityLabels([360,480,720,1080]),['360p','480p','720p','1080p']);
  assert.deepEqual(qualityLabels([1080]),['1080p']);
  assert.deepEqual(qualityLabels([2160]),[]);
});
test('login polling ends after confirmation, expiry or failure',()=>{
  assert.ok(loginPending('verifying'));assert.ok(loginPending('scanned'));
  for(const state of ['signed_in','expired','failed','idle'])assert.equal(loginPending(state),false);
});
test('episode navigation follows only the confirmed available list',()=>{
  assert.equal(adjacent([1,3,7],3,1),7);assert.equal(adjacent([1,3,7],1,-1),undefined);
  assert.equal(adjacent([1,3,7],2,1),undefined);assert.equal(message('login_required'),'请先登录红果账号');
});
test('playback failures stay readable when wrapped as Error objects',()=>{
  assert.equal(message(new Error('app_playback_empty_response')),'App 播放接口返回空内容（HTTP 200），尚未获取视频资源');
  assert.equal(message(new Error('app_access_denied')),'当前账号无法访问该媒体');
});
