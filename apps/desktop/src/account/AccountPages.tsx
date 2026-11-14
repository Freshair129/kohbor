import { accountStateText, type AccountView } from "./accountView";
import "./account.css";

export function AccountPage({ account, locale, back }: { account: AccountView; locale: "th" | "en"; back: () => void }) {
  const th = locale === "th";
  return <section className="account-page" aria-labelledby="page-account-heading">
    <header className="account-page-heading"><div><small>ACCOUNT</small><h1 id="page-account-heading">{th ? "บัญชีของฉัน" : "My account"}</h1>
      <p>{th ? "บัญชีเกาะเบาะใช้จัดการตัวตน ไม่ใช่การยินยอมแชร์ข้อมูล" : "Your Kohbor identity is separate from consent to share data."}</p></div>
      <button type="button" onClick={back}>{th ? "← กลับหน้าสด" : "← Back to Live"}</button></header>
    <div className="account-page-grid">
      <section className="account-card"><div className="account-identity"><span className="account-avatar" aria-hidden="true">?</span><h2>{accountStateText(account, locale)}</h2></div>
        <button type="button" disabled>{th ? "เข้าสู่ระบบด้วย Google" : "Sign in with Google"}</button><p>{th ? "บริการยังไม่เปิด" : "Service not available yet"}</p>
        <dl><dt>{th ? "ชื่อที่แสดง" : "Display name"}</dt><dd>{account.displayName ?? "—"}</dd></dl>
      </section>
      <section className="account-card"><h2>{th ? "รหัสเพื่อน" : "Friend code"}</h2><p className="account-unknown">—</p><p>{th ? "ยังไม่เปิดใช้ในรุ่นนี้" : "Not available in this version"}</p>
        <hr /><h2>{th ? "คุณเป็นคนเลือกสิ่งที่แชร์" : "You choose what to share"}</h2><p>{th ? "การเข้าสู่ระบบไม่แชร์ข้อมูลอะไร" : "Signing in does not share any data."}</p>
      </section>
      <section className="account-card"><h2>{th ? "การเชื่อมต่อ" : "Connection"}</h2><p>{accountStateText(account, locale)}</p>
        <p>{th ? "บัญชีเกาะเบาะกับ Steam เป็นคนละบัญชี" : "Kohbor and Steam are separate accounts."}</p><hr />
        <h2>{th ? "บนเครื่องนี้" : "On this computer"}</h2><p>{th ? "ตั้งค่าและสถานะเกมยังอยู่ในเครื่องตามเดิม" : "Settings and game status remain local."}</p>
        <button type="button" disabled>{th ? "ออกจากระบบ" : "Sign out"}</button>
      </section>
    </div>
  </section>;
}
export function FriendsPage({ locale, back }: { locale: "th" | "en"; back: () => void }) {
  const th = locale === "th";
  return <section className="friends-page" aria-labelledby="page-friends-heading">
    <header className="account-page-heading"><div><small>FRIENDS</small><h1 id="page-friends-heading">{th ? "เพื่อน" : "Friends"} · —</h1>
      <p>{th ? "ยังไม่มีข้อมูลจำนวนเพื่อน จึงไม่แสดงเป็นศูนย์" : "Friend count is unknown."}</p></div><button type="button" onClick={back}>{th ? "← กลับหน้าสด" : "← Back to Live"}</button></header>
    <div className="friends-page-grid"><aside className="account-card"><h2>{th ? "คนที่คุณเลือก" : "People you choose"}</h2>
      <p>{th ? "เพื่อนจะอยู่ตรงนี้เมื่อบริการพร้อม" : "Your friends will appear here when the service is available."}</p><hr />
      <button type="button" disabled>{th ? "เพิ่มเพื่อน" : "Add friend"}</button><p>{th ? "เร็วๆ นี้ — ยังไม่เปิดบริการ" : "Coming soon — service unavailable"}</p>
      <small>{th ? "การเข้าสู่ระบบไม่แชร์ข้อมูลอะไร" : "Signing in does not share any data."}</small></aside>
      <section className="account-card friends-empty"><span className="friends-empty-mark" aria-hidden="true">—</span><small>{th ? "เร็วๆ นี้" : "Coming soon"}</small>
        <h2>{th ? "พื้นที่สำหรับเพื่อนของคุณ" : "A place for your friends"}</h2><p>{th ? "ยังไม่เปิดระบบเพื่อน ไม่มีรายชื่อหรือสถานะที่ต้องเดา" : "Friends are not available yet. No names or presence are inferred."}</p></section>
    </div>
  </section>;
}
