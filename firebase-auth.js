/* Firebase Auth cho PWA sync (2026-11-27).
 *
 * Chỉ dùng Firebase JS SDK cho ĐĂNG NHẬP Google — Firestore vẫn đi
 * REST qua `fetch` (xem `xq-iced/src/sync_fetch.rs`), vì REST đã có
 * CORS từ Google và dùng chung code với desktop.
 *
 * - `signInWithPopup` thử trước; iOS standalone PWA hay chặn popup
 *   nên rớt thì dùng `signInWithRedirect` (trang sẽ tải lại, phiên
 *   được khôi phục qua `onAuthStateChanged` khi mở lại).
 * - SDK tự giữ phiên trong IndexedDB và tự refresh id_token, nên
 *   Rust không cần quản refresh token như desktop (keychain).
 * - Mọi hàm trả về Promise<string> là JSON `{ok, ...}` — không bao
 *   giờ throw qua bridge, để Rust chỉ cần parse JSON.
 *
 * Config web public by-design (như `xq-sync/src/config.rs`): API key
 * chỉ định danh project; bảo mật thật ở Firestore rule chỉ-owner.
 * Domain `olivanderwood.github.io` phải có trong Firebase console →
 * Authentication → Settings → Authorized domains thì popup/redirect
 * trên site live mới chạy (localhost được phép sẵn).
 */

import { initializeApp } from "https://www.gstatic.com/firebasejs/11.10.0/firebase-app.js";
import {
  getAuth,
  GoogleAuthProvider,
  onAuthStateChanged,
  signInWithPopup,
  signInWithRedirect,
  signOut,
} from "https://www.gstatic.com/firebasejs/11.10.0/firebase-auth.js";

const app = initializeApp({
  apiKey: "AIzaSyApLltjDd2MK--J2WBsCGomshJA5wqGKoM",
  authDomain: "xiangqi-sync.firebaseapp.com",
  projectId: "xiangqi-sync",
});

const auth = getAuth(app);
const provider = new GoogleAuthProvider();

// Promise chờ auth "settle" lần đầu (SDK khôi phục phiên từ
// IndexedDB là bất đồng bộ — `currentUser` đọc ngay lúc mở app có
// thể còn null dù người dùng đã đăng nhập trước đó).
let settledResolve;
const settled = new Promise((resolve) => {
  settledResolve = resolve;
});
onAuthStateChanged(auth, () => {
  if (settledResolve) {
    settledResolve();
    settledResolve = null;
  }
});

function userJson(user) {
  if (!user) return null;
  return { uid: user.uid, email: user.email || "" };
}

function ok(data) {
  return JSON.stringify({ ok: true, ...data });
}

function fail(error) {
  const message =
    (error && (error.message || error.code || String(error))) ||
    "loi khong ro";
  return JSON.stringify({ ok: false, error: message });
}

window.__xiangqiAuth = {
  // Chờ SDK khôi phục phiên xong rồi trả user hiện tại (null nếu chưa
  // đăng nhập).
  async waitForAuth() {
    try {
      await settled;
      return ok({ user: userJson(auth.currentUser) });
    } catch (e) {
      return fail(e);
    }
  },

  // User hiện tại (đọc ngay, có thể null nếu SDK chưa settle — dùng
  // `waitForAuth` khi cần chắc chắn).
  async currentUser() {
    try {
      return ok({ user: userJson(auth.currentUser) });
    } catch (e) {
      return fail(e);
    }
  },

  // Id token còn hạn (SDK tự refresh nếu hết — không gọi mạng khi
  // token vẫn còn hạn). Lỗi khi chưa đăng nhập.
  async getIdToken() {
    try {
      const user = auth.currentUser;
      if (!user) return fail("chua dang nhap");
      const idToken = await user.getIdToken(false);
      return ok({ idToken });
    } catch (e) {
      return fail(e);
    }
  },

  // Đăng nhập Google: popup trước; rớt vì popup bị chặn / không hỗ
  // trợ thì redirect (trang tải lại, phiên khôi phục lúc mở lại).
  // Người dùng tự đóng popup thì coi như huỷ, không redirect.
  async signIn() {
    try {
      const cred = await signInWithPopup(auth, provider);
      const idToken = await cred.user.getIdToken(false);
      return ok({
        uid: cred.user.uid,
        email: cred.user.email || "",
        idToken,
      });
    } catch (e) {
      const code = (e && e.code) || "";
      if (
        code === "auth/popup-blocked" ||
        code === "auth/operation-not-supported" ||
        code === "auth/cancelled-popup-request" ||
        code === "auth/popup-closed-by-user"
      ) {
        if (code === "auth/popup-closed-by-user") {
          return fail("da dong cua so dang nhap");
        }
        // Redirect: trang sẽ rời đi ngay; trả cờ để Rust biết là
        // không chờ nữa (phiên sẽ khôi phục khi tải lại).
        signInWithRedirect(auth, provider).catch(() => {});
        return ok({ redirecting: true });
      }
      return fail(e);
    }
  },

  async signOut() {
    try {
      await signOut(auth);
      return ok({});
    } catch (e) {
      return fail(e);
    }
  },
};
