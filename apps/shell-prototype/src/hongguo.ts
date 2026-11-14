export type Item = {id:string;title:string;episodes:number|null;has_cover?:boolean};
export type Account = {state:string;name:string;qr_png?:string;error?:string|null;favorites_error?:string|null;favorites?:Item[];operation?:string|null};
export type Playback = {session:number;loading:boolean;prepared:boolean;suspended:boolean;resource:{url:string;mime:string}|null;episodes?:number[];available_qualities?:string[];error?:string|null;progress_error?:string|null};
export function loginPending(state:string):boolean {return ['preparing','pending','scanned','verifying'].includes(state);}
export function adjacent(episodes:number[],current:number,delta:number):number|undefined {
  const index=episodes.indexOf(current);
  return index<0?undefined:episodes[index+delta];
}
export function message(error:unknown):string {
  const value=String(error);
  return ({login_required:'请先登录红果账号',login_step_failed:'扫码登录失败，请重试',credential_save_failed:'登录信息安全保存失败',
    saved_session_verification_failed:'已保存的登录状态核对失败，请重新登录',favorites_unavailable:'收藏列表读取失败',favorites_sync_failed:'收藏同步失败',
    app_access_denied:'当前账号无法访问该媒体',episode_unavailable:'当前集不可播放',quality_unavailable:'当前清晰度不可用',
    media_access_denied:'媒体访问被拒绝',media_network_error:'媒体网络请求失败',app_source_network_error:'取源网络请求失败',
    app_source_timeout:'取源超时',playback_service_unavailable:'红果服务连接已断开，请重试'} as Record<string,string>)[value]??value;
}
