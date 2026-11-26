# Spike Phase 0 — Sync mirroring (Firebase + Google, qua REST)

Crate test riêng, ngoài app chính. Kiểm chứng 4 việc trước khi code thật
(plan `~/workspace/research_notes/sync-plan-20261125/plan.md` §Phase 0):

1. Đăng nhập Google bằng OAuth loopback (Desktop app).
2. Đổi Google `id_token` lấy Firebase `idToken` + `refreshToken`.
3. Ghi 1 document ván cờ lên Firestore rồi đọc lại (roundtrip).
4. Đổi `refreshToken` lấy `idToken` mới.

HTTP client: **ureq 3.5.2 + rustls** (pure Rust TLS, không OpenSSL).
`reqwest` là dự phòng nếu ureq 3 gặp vấn đề trên Windows — ureq 2.x từng
gây crash `0xc0000409` trong repo này (xq-chessdb), nên spike này PHẢI
chạy qua trên máy Windows của Hieu trước khi chốt.

## 1. Tạo project Firebase (làm 1 lần, trên tài khoản Google của Hieu)

1. Mở https://console.firebase.google.com → **Add project** → tên
   `xiangqi-sync` → tắt Google Analytics (không cần) → Create project.
2. **Authentication** → **Sign-in method** → bật **Google** → Save.
3. **Firestore Database** → **Create database** → chọn **Production mode**,
   location gần nhất (vd `asia-southeast1`) → Create.
4. Tab **Rules** của Firestore, dán rule dưới đây rồi **Publish**:

```js
rules_version = '2';
service cloud.firestore {
  match /databases/{database}/documents {
    match /users/{userId}/{document=**} {
      allow read, write: if request.auth != null && request.auth.uid == userId;
    }
  }
}
```

5. **Project settings** (bánh răng) → **General** → **Your apps** → bấm
   biểu tượng Web `</>` → đặt tên `xiangqi` → Register app → copy
   **`apiKey`** và **`projectId`** trong đoạn config hiện ra.
6. Tạo OAuth client cho app desktop (Firebase tự tạo 1 Web client, KHÔNG
   dùng được cho loopback):
   - Mở https://console.cloud.google.com/apis/credentials — chắc chắn đang
     ở đúng project `xiangqi-sync` (Firebase đã tạo sẵn Google Cloud
     project cùng tên).
   - **Create Credentials** → **OAuth client ID** → Application type:
     **Desktop app** → tên `xiangqi-desktop` → Create → copy
     **`Client ID`** và **`Client secret`**.
   - (Desktop app được phép dùng mọi redirect `http://127.0.0.1:<port>`
     mà không cần đăng ký trước.)

## 2. Chạy spike

Máy Windows (Command Prompt, trong thư mục `tools\sync-spike`):

```bat
set FIREBASE_API_KEY=<apiKey o buoc 5>
set FIREBASE_PROJECT_ID=<projectId o buoc 5>
set GOOGLE_CLIENT_ID=<Client ID o buoc 6>
set GOOGLE_CLIENT_SECRET=<Client secret o buoc 6>
cargo run --release
```

Máy Mac/Linux: dùng `export VAR=value` thay cho `set`.

Spike sẽ mở trình duyệt để đăng nhập Google, rồi in từng bước `[1/4]` …
`[4/4]`. Chạy qua hết = Phase 0 xong phần mạng; còn lại là quyết định
HTTP client để ghi vào `docs/DECISIONS-sync.md`.

## 3. Lưu ý

- API key của Firebase nhúng trong app là **public by-design** — bảo mật
  thật nằm ở Firestore rule ở trên (chỉ chủ tài khoản đọc/ghi dữ liệu
  của mình). Không bao giờ nhúng service key / admin key vào app.
- Token chỉ in ra màn hình ở dạng tóm tắt (uid/email), không log token.
- Spike này chỉ chạy trên máy có trình duyệt. VM Linux chỉ build được,
  không chạy hết flow (không có trình duyệt để đăng nhập Google).
- Document mẫu ghi lên cloud: `users/{uid}/games/{uuid}` — đúng đường
  dẫn Phase 1 sẽ dùng. Xóa tay trên console nếu muốn dọn.
