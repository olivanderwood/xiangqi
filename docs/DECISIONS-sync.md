# Quyết định — sync mirroring (Firebase + Google)

Plan gốc: `~/workspace/research_notes/sync-plan-20261125/plan.md` (2026-11-25).
File này ghi các quyết định đã chốt và kết quả spike Phase 0.
KHÔNG tự commit/push.

## 0. Trạng thái (2026-11-27)

- Spike Phase 0 **ĐÃ QUA trên máy Windows của Hieu (2026-11-27 08:51)**:
  cả 4 bước đều chạy — đăng nhập Google + Firebase Auth + Firestore
  ghi/đọc 1 document ván cờ (roundtrip khớp) + refresh token. Phase 0
  xong, verdict HTTP client đã chốt ở §5. Code spike ở
  `tools/sync-spike/`, đã push lên nhánh `pwa-spike` (4d7f7f7).
- Project Firebase `xiangqi-sync` đã tạo xong 2026-11-27 00:29
  (tài khoản hieuvd1412@gmail.com, Spark $0).
- **Phase 1 ĐANG LÀM (2026-11-27, chưa commit, chưa push — chờ lệnh
  riêng)**: nền tảng xong (§8) + worker/UI xong (§9). Còn lại: test
  Play-sync (apply remote), cổng cuối (full test + binary release
  < 90 MB), rồi chờ Hieu test 2 máy thật.

## 1. Backend: Firebase (Spark, $0) — CHỐT 2026-11-25

Plan §4 so sánh 5 phương án. Firebase thắng vì: không cần thẻ, không
server để vận hành, không bị pause sau 1 tuần như Supabase Free, quota
(Auth 50k MAU, Firestore 1 GiB / 50k reads / 20k writes mỗi ngày) dư sức
cho 1 người dùng. Supabase/PocketBase chỉ còn là dự phòng.

## 2. Đăng nhập: chỉ Google — CHỐT 2026-11-25

Hieu từ chối email/mật khẩu outright. Apple chưa làm được khi chưa có
tài khoản Developer $99/năm (plan §4.1). Luồng desktop: OAuth loopback
(`http://127.0.0.1:<port>`, Desktop app client), đổi Google `id_token`
lấy Firebase token qua REST `accounts:signInWithIdp`.

## 3. Phạm vi phase 1: chỉ ván Đấu với AI đang dở + lịch sử — CHỐT 2026-11-25

Học khai cuộc + Tự luyện tập để phase sau (plan §7.4).

## 4. Luật xung đột: mirroring trực tiếp, không khóa mềm — CHỐT 2026-11-25

Hai máy cùng mở một ván là CÙNG một ván: nước đi ở máy này hiện ngay ở
máy kia (listener). Nước trả của AI chỉ MỘT máy chạy (máy nơi người vừa
đi); máy kia chỉ áp nước AI khi nhận được. Hai máy cùng đi một lượt:
nước tới cloud trước (theo `version`) thắng, máy kia hiện "Máy kia vừa
đi trước". Ván dở KHÁC hẳn: giữ cả hai (mới hơn làm ván đang dở, cũ vào
Lịch sử dạng bản lưu trữ). Chi tiết plan §5.4.

## 5. HTTP client: ureq 3.5.2 + rustls — CHỐT 2026-11-27

Plan §Phase 0 yêu cầu spike quyết định giữa `reqwest` + rustls (nặng
nhưng chắc) và `ureq` + rustls (nhẹ). Lý do chọn ureq 3 làm ứng viên:

- `ureq` 2.x từng gây crash `0xc0000409` trên Windows trong repo này
  (xq-chessdb, xem `crates/xq-chessdb/Cargo.toml`) nên bản 2 bị loại;
  ureq 3 là viết lại, API khác hẳn.
- ureq 3.5.2 + rustls build được trên VM (cargo tải được rustls 0.23.46
  + ring từ crates.io), binary spike 3,6 MB — rất xa trần 90 MB.
- rustls là TLS thuần Rust, không cần OpenSSL trên Windows.

**Verdict CHỐT 2026-11-27: dùng ureq 3.5.2 + rustls.** Bằng chứng:
spike `tools/sync-spike/` chạy qua hết 4 bước trên máy Windows của Hieu
(đăng nhập Google + Firebase Auth + Firestore ghi/đọc roundtrip +
refresh token), không crash. `reqwest` không còn cần làm dự phòng.

## 6. Spike kiểm chứng những gì

`tools/sync-spike/src/main.rs`, cấu hình qua biến môi trường
(`FIREBASE_API_KEY`, `FIREBASE_PROJECT_ID`, `GOOGLE_CLIENT_ID`,
`GOOGLE_CLIENT_SECRET` — không hardcode secret):

1. Google OAuth loopback → Google `id_token`.
2. `POST identitytoolkit.googleapis.com/v1/accounts:signInWithIdp`
   (`postBody: id_token=...&providerId=google.com`) → Firebase
   `idToken` + `refreshToken` + `localId` (uid).
3. Firestore REST: `PATCH .../documents/users/{uid}/games/{uuid}`
   (upsert, typed values: `stringValue`/`integerValue`/`booleanValue`/
   `arrayValue`) rồi `GET` lại kiểm tra roundtrip (`moves` khớp).
4. `POST securetoken.googleapis.com/v1/token`
   (`grant_type=refresh_token`) → `idToken` mới, đọc lại document được.

Firestore rule (chỉ owner đọc/ghi), xem `tools/sync-spike/README.md` §1.
Đường dẫn document `users/{uid}/games/{uuid}` là đường dẫn Phase 1 sẽ dùng.

## 7. Việc tiếp theo (sau khi spike qua trên Windows)

1. Ghi verdict HTTP client vào §5 ở trên. — XONG 2026-11-27.
2. Phase 1 theo plan §Phase 1: `SavedGame` thêm field sync (`uuid`,
   `updated_unix`, `updated_by_device`, `version`, `deleted`); crate mới
   `xq-sync` (trait `SyncBackend`, Firebase implement); tự lưu thầm sau
   mỗi nước; UI Tài khoản trong Cài đặt; hộp thoại "Tiếp tục ván đang dở";
   migrate lần đầu theo plan §5.5. — XONG, xem §8–§9.
3. Cổng theo chuẩn repo: `cargo fmt`, clippy `-D warnings`, full test
   (thêm test roundtrip field sync, xung đột giữ cả hai bản, mất mạng vẫn
   chơi được + hàng đợi đẩy lại khi có mạng), binary release < 90 MB.
   — ĐANG LÀM: fmt + clippy + wasm check xanh; full test đang chạy.

## 8. Phase 1 — nền tảng (2026-11-27)

- Crate mới `crates/xq-sync` (workspace member): `auth.rs` (Google
  OAuth loopback → `signInWithIdp` → refresh; chờ redirect tối đa 5
  phút, ureq timeout toàn cục 30 s), `firestore.rs` (upsert/get/list,
  typed values), `backend.rs` (trait `SyncBackend` + `FirebaseBackend`
  + `MockBackend` in-memory cho test), `conflict.rs` (cùng ván: version
  cao thắng, hoà version thì `updated_unix` mới thắng, hoà cả hai thì
  remote thắng; ván khác: `(updated_unix, version)` lớn hơn làm ván
  đang dở), `model.rs` (`SyncEntry`). Tổng 19 test (model 3, firestore 5, backend 3,
  conflict 6, auth 2).
- Cấu hình Firebase (`XIANGQI_SYNC_CONFIG`) chỉ nhúng 2 giá trị public
  vào `crates/xq-sync/src/config.rs`: Web API key (public by-design)
  + project ID; bảo mật thật nằm ở Firestore rule chỉ-owner. OAuth
  client ID + client secret (loại Desktop) KHÔNG nằm trong repo:
  GitHub push protection chặn push khi thấy pattern
  `GOOGLE_OAUTH_CLIENT_ID` / `GOOGLE_OAUTH_CLIENT_SECRET` (gặp lần
  đầu lúc push nhánh `pwa-spike` 2026-11-27, lỗi 422 "Secret detected
  in content"), và blob wasm trên gh-pages cũng bị chặn theo. Nạp lúc
  chạy qua `OAuthCredentials::load` — biến môi trường
  `GOOGLE_CLIENT_ID` + `GOOGLE_CLIENT_SECRET` (giống spike Phase 0)
  hoặc file `sync-oauth.json` cạnh `settings.json`
  (`{"client_id": "...", "client_secret": "..."}`); thiếu thì chỉ
  đăng nhập Google báo lỗi hướng dẫn cách tạo, còn khôi phục phiên
  bằng refresh token (chỉ cần API key) vẫn chạy. PWA không cần file
  này (web SDK chỉ dùng apiKey + projectId).
- `SavedGame` thêm 6 field sync (`uuid`, `client_id`, `version`,
  `updated_unix`, `updated_by_device`, `deleted`, đều `#[serde(default)]`
  để file cũ đọc được); `migrate_sync()` cấp UUID v4 lúc load (giữ id
  cũ trong `client_id`, version 1, `updated_unix = created_unix`);
  `to_sync_entry` / `from_sync_entry` (id của entry đồng bộ == uuid để
  2 máy dedup đúng); ván mới tạo có uuid sẵn (`fresh_sync_uuid`,
  wasm-safe — trả chuỗi rỗng trên wasm).
- `Settings` thêm `device_id` (`device-<uuid>`, cấp lúc load rồi ghi lại
  ngay để ổn định), `device_name` (mặc định "Máy này", đổi được trong
  Cài đặt), `sync_email` (chỉ hiển thị).

## 9. Phase 1 — worker + UI (2026-11-27)

- `crates/xq-iced/src/sync.rs` (desktop-only thread, wasm là vỏ rỗng):
  `SyncWorker` + `SyncRequest` / `SyncResponse` theo mẫu `ai_worker.rs`;
  token qua `keyring` 3.6.3 (service `xiangqi-sync`, 2 entry
  `refresh-token` + `email` — KHÔNG vào settings.json); id_token chỉ
  sống trong bộ nhớ worker, uid sau restore lấy từ JWT payload
  (`decode_jwt_user_id`, chỉ để hiển thị/làm key — token vừa lấy qua
  TLS từ Firebase).
- Worker tự làm sau Restore/SignIn thành công: pull toàn bộ ván
  (`PulledAll`). Push thì pull-trước-push-sau: remote thắng theo
  `resolve_same_game` thì trả `PushRejected` (UI áp bản remote);
  lỗi mạng thì `PushFailed` (UI đẩy lại sau, tối đa 5 giây/lần —
  mất mạng vẫn chơi được).
- Poll ván đang xem mỗi 5 giây (`POLL_INTERVAL`); remote mới hơn
  (version cao hơn, hoặc cùng version nhưng nước khác) thì gửi
  `RemoteUpdate`. Tombstone (`deleted`) báo một lần rồi dừng theo dõi.
- `App` drain response 2 lần/giây (`Message::SyncPoll`): phiên
  (SignedIn/Failed/Out) cập nhật `Shared::sync` + `settings.sync_email`;
  `PulledAll` thì đề nghị tiếp tục ván nếu cloud có ván Play đang dở
  mới hơn ván dở local (Home hiện card "Tiếp tục ván này / Để sau";
  chấp nhận thì ván cũ vào Lịch sử dạng bản lưu trữ); game-level
  (Pushed/PushRejected/PushFailed/RemoteUpdate) chuyển cho màn Play.
- Màn Play: `sync_entry` (uuid ổn định, version +1 mỗi lần đẩy, id ==
  uuid), đẩy sau mỗi nước ở Tick (sau khi quân hạ xuống, như mọi work
  khác), Undo/Resign/hoà/hết giờ Focus cũng đánh dấu dirty. Áp nước
  remote: nối tiếp thì chỉ đi nước mới (nước cuối có animation + tiếng);
  lệch nhau thì rebuild cả ván + banner "Máy kia vừa đi trước".
  Nước trả AI chỉ một máy chạy: sau khi áp nước remote mà tới lượt AI
  thì `sync_waiting_remote_ai` chặn AI local, chờ `RemoteUpdate`.
  Ván kết thúc thì đẩy trạng thái cuối rồi dừng theo dõi. Chỉ Đấu với
  AI mới sync (book game / 2 người không sync).
- Cài đặt → Tài khoản đồng bộ (desktop only): đăng nhập bằng Google
  (mở trình duyệt, chờ tối đa 5 phút), hiện email, tên máy, đăng xuất.
- Chưa test 2 máy thật: toàn bộ mirroring mới chỉ test bằng
  `MockBackend` (push/pull/reject/restore/sign-out) — Hieu cần test
  trên Windows + iPhone (PWA chưa sync) sau khi push.

## 10. PWA sync (2026-11-27, Hieu ra lệnh "làm sync cho pwa luôn đi")

- **CORS đã kiểm chứng bằng curl**: cả 3 endpoint Google đều trả
  `Access-Control-Allow-Origin: https://olivanderwood.github.io`
  trên preflight (Firestore REST, Identity Toolkit, Secure Token) —
  PWA gọi thẳng REST qua `fetch`, KHÔNG cần proxy, KHÔNG cần
  Firebase JS SDK cho Firestore.
- **Auth web**: Firebase JS SDK **v11.10.0** (CDN gstatic, đã kiểm
  200 cho cả `firebase-app.js` và `firebase-auth.js`) CHỈ để đăng
  nhập Google: `signInWithPopup` (thử trước) → rớt thì
  `signInWithRedirect` (iOS standalone PWA hay chặn popup);
  `onAuthStateChanged` để khôi phục phiên; `getIdToken()` để lấy
  id_token (SDK tự refresh, Rust không cần quản refresh token).
  Glue JS nhỏ `assets/pwa/firebase-auth.js`, mọi hàm trả JSON
  string `{ok, ...}` (không throw qua bridge).
- **Kiến trúc**: KHÔNG refactor async toàn bộ xq-sync (giữ code
  desktop đã qua cổng). `xq-sync` tách phần pure của Firestore
  (`doc_url`/`collection_url` → pub, `entry_to_fields`/`parse_entry`
  đã pub) cho worker wasm dùng chung; `ureq` + loopback OAuth +
  `FirebaseBackend` gate non-wasm; `uuid` thêm feature `js` cho
  wasm (v4 qua `crypto.getRandomValues`). Worker wasm là async loop
  riêng (`spawn_local`, vẫn dùng `std::sync::mpsc` như desktop —
  channel không cần thread), mirror đúng logic desktop
  (push pull-trước, poll 5 s, PushRejected, RemoteUpdate,
  tombstone) nhưng transport là `fetch` (gloo-net) + auth qua JS
  bridge. `fresh_sync_uuid` / `migrate_sync` / `ensure_device_id`
  mở cho cả wasm. App gửi RestoreSession + Cài đặt hiện mục Tài
  khoản trên cả 2 target.
- **Token trên web**: SDK giữ phiên trong IndexedDB; Rust chỉ giữ
  id_token/uid/email trong bộ nhớ worker (refresh_token rỗng).
- **Lưu trữ bền trên PWA** (`xq-iced/src/storage.rs`, phát hiện khi
  làm sync): desktop dùng file JSON nhưng trên wasm không có
  filesystem (`std::fs` luôn lỗi, được nuốt lặng lẽ) — settings,
  tên máy, `device_id` và ván lưu vốn mất mỗi lần tải lại trang.
  Sync cần id máy + ván lưu ổn định giữa các lần mở nên settings
  và saved games đi qua localStorage trên wasm (key
  `xiangqi-settings` / `xiangqi-saved-games`, ~vài chục KB, dưới
  quota 5 MB). Nhờ đó ván dở vẫn còn trong Lịch sử sau khi tải lại
  và không bị coi là "máy mới" mỗi lần mở.
- **Bridge extern phải là sync Promise, không `async fn`** (bắt
  được 2026-11-27 khi kiểm headless với gstatic bị chặn): với
  `catch` + `async fn` trong extern, khi hàm JS throw đồng bộ (glue
  chưa load) thì wrapper trả `undefined` thay vì Err, rồi machinery
  await gọi `.then` lên `undefined` và ném lỗi không bắt được —
  app trắng màn hình lúc mở. `firebase_auth.rs` dùng extern đồng
  bộ trả `js_sys::Promise` + `JsFuture` (`call_glue`); glue thiếu
  thì về Err, app vẫn chạy, sync chỉ ở Off. Đã kiểm chứng: stub
  glue thì canvas hiện, thiếu glue (bản cũ) thì trắng màn hình.
- **Kiểm headless 2026-11-27 (sau sửa, build Trunk mới
  `xiangqi-iced-1c917b999df2d9d3_bg.wasm`)**: trình duyệt VM không
  ra được internet nên SDK thật + popup Google + fetch Firestore
  từ trang KHÔNG kiểm được ở đây (chỉ kiểm bằng curl: preflight
  Firestore trả CORS cho origin PWA). Kiểm được: app render
  1280×800 và 390×844 khi SDK không tải được, không lỗi không bắt
  được; dialog "Ván mới" mở, bàn cờ hiện quân; Cài đặt mở, mục
  **Tài khoản đồng bộ** hiện đúng ("Đồng bộ lỗi: Firebase Auth
  chua san sang — kiem tra mang roi thu lai" + nút Đăng nhập bằng
  Google); bấm đăng nhập khi glue thiếu thì lỗi êm (không popup,
  dialog vẫn mở, vẫn hiện lỗi). **Đăng nhập thật + đồng bộ 2 máy
  trên PWA cần Hieu test trên iPhone sau khi deploy.**
- **Chưa làm**: `onSnapshot` realtime (giữ poll 5 s như desktop —
  đủ nhanh cho ván cờ, để phase sau nếu cần); ChessDB web vẫn tắt.
- **Hieu cần làm 1 bước tay ở Firebase console**: thêm
  `olivanderwood.github.io` vào Authentication → Settings →
  Authorized domains (localhost đã được phép sẵn nên test local
  không cần). Deploy gh-pages vẫn nhờ anh push file wasm từ máy
  anh (file > 50 MB không qua được GitHub API từ VM).
