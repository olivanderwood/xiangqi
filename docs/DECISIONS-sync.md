# Quyết định — sync mirroring (Firebase + Google)

Plan gốc: `~/workspace/research_notes/sync-plan-20261125/plan.md` (2026-11-25).
File này ghi các quyết định đã chốt và kết quả spike Phase 0.
KHÔNG tự commit/push.

## 0. Trạng thái (2026-11-26)

- Spike Phase 0 **đã viết xong, chưa chạy hết**: `tools/sync-spike/`
  (crate riêng, tách khỏi workspace chính). Build sạch trên VM Linux
  (`cargo fmt`, `cargo clippy --all-targets -- -D warnings`), binary
  release 3,6 MB. Chạy không biến môi trường thì báo thiếu
  `FIREBASE_API_KEY` đúng như thiết kế.
- **Đang chờ Hieu**: tạo project Firebase (cần tài khoản Google của anh)
  rồi chạy spike trên máy Windows (cần trình duyệt để đăng nhập Google).
  Hướng dẫn tạo project ở `tools/sync-spike/README.md`.

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

## 5. HTTP client: ureq 3.5.2 + rustls (ứng viên) — CHƯA CHỐT

Plan §Phase 0 yêu cầu spike quyết định giữa `reqwest` + rustls (nặng
nhưng chắc) và `ureq` + rustls (nhẹ). Lý do chọn ureq 3 làm ứng viên:

- `ureq` 2.x từng gây crash `0xc0000409` trên Windows trong repo này
  (xq-chessdb, xem `crates/xq-chessdb/Cargo.toml`) nên bản 2 bị loại;
  ureq 3 là viết lại, API khác hẳn.
- ureq 3.5.2 + rustls build được trên VM (cargo tải được rustls 0.23.46
  + ring từ crates.io), binary spike 3,6 MB — rất xa trần 90 MB.
- rustls là TLS thuần Rust, không cần OpenSSL trên Windows.

**Verdict CHƯA có**: spike phải chạy qua trên máy Windows của Hieu
(đăng nhập Google thật + ghi/đọc Firestore thật) mới chốt. Nếu ureq 3
gặp vấn đề trên Windows thì đổi sang `reqwest` + rustls và ghi lại ở đây.

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

1. Ghi verdict HTTP client vào §5 ở trên.
2. Phase 1 theo plan §Phase 1: `SavedGame` thêm field sync (`uuid`,
   `updated_unix`, `updated_by_device`, `version`, `deleted`); crate mới
   `xq-sync` (trait `SyncBackend`, Firebase implement); tự lưu thầm sau
   mỗi nước; UI Tài khoản trong Cài đặt; hộp thoại "Tiếp tục ván đang dở";
   migrate lần đầu theo plan §5.5.
3. Cổng theo chuẩn repo: `cargo fmt`, clippy `-D warnings`, full test
   (thêm test roundtrip field sync, xung đột giữ cả hai bản, mất mạng vẫn
   chơi được + hàng đợi đẩy lại khi có mạng), binary release < 90 MB.
