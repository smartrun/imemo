# iMemo

**iMemo**는 클라우드 PC(원격 데스크톱 등) 환경에서 작성한 서식 문서와 첨부 이미지를 파일 다운로드 없이 **가로형 비표준 매트릭스 코드(IMC)**로 화면에 렌더링하고, macOS 및 Windows에서 **화면 특정 영역 캡처**를 통해 원본 서식과 이미지 그대로 고속 복원하는 데이터 브리지 시스템입니다.

---

## 📌 주요 특징

### 1. 클라우드 PC 웹 에디터 (`web/index.html`)
- **WYSIWYG 서식 편집**: 제목(H1~H3), 굵게, 기울임, 밑줄, 취소선, 글머리/번호 목록, 표 삽입 등 지원.
- **이미지 첨부 및 클립보드 지원**:
  - 툴바 **[🖼️ 이미지 첨부]** 버튼
  - 화면 캡처 클립보드 붙여넣기 (**`Ctrl + V`**)
  - 이미지 파일 드래그 앤 드롭
  - **스마트 WebP 자동 경량화**: 원본 이미지를 15~35KB(640~1024px) 수준으로 자동 압축하여 전송 효율 극대화.
- **가로 해상도 동적 확장 (Chrome 최적화)**:
  - 800px ~ 1920px(FHD) 및 모니터 가로폭 맞춤 지원.
  - 와이드 모니터에서도 가로 스크롤바 없이 캔버스가 시원하게 확장 표시.
  - 32비트 직접 픽셀 버퍼링(`putImageData`)으로 서브픽셀 번짐 없이 셀 경계가 칼같이 선명하게 렌더링.
- **다양한 색상 모드**: 2컬러(흑백 기본 - 노이즈 최강), 4컬러(2비트), 8컬러(3비트), 16컬러(4비트 - 이미지 권장, 용량 4배).
- **편의 기능**:
  - **⛶ 편집창 최대화**: 브라우저 전체 화면 몰입형 작성 모드 (ESC로 복귀).
  - **코드 접기/펼치기**: 바코드를 깔끔하게 접어두고 문서 작성에 집중 가능.
  - **🧹 서식 지우기**: 선택 텍스트 또는 문서 전체의 서식을 일반 텍스트로 일괄 초기화.
  - **⛶ Chrome 전체화면 확대**: 탭/주소창을 숨긴 순수 전체화면 캡처 모드.
- **대용량 슬라이드 모드**:
  - 한 화면 용량을 초과하는 문서는 N장의 프레임으로 자동 분할.
  - **25% XOR 패리티 복구 프레임**을 자동 생성하여 순환 재생 (프레임 드롭 발생 시 누락 자동 복구).
- **100% 오프라인 동작**: 외부 라이브러리나 인터넷 연결 없이 단일 HTML 파일로 동작.

### 2. Windows 네이티브 리더기 앱 (`ImemoReaderWin`, Rust)
- **Windows 10/11 네이티브 초경량 초고속 지원**: Rust 언어 기반 단일 독립 실행 파일 (`imemo-reader-win.exe`).
- **Win32 GDI & BitBlt 초저지연 화면 캡처**:
  - 1~2ms 수준의 화면 특정 영역 캡처.
  - **Per-Monitor V2 DPI 인식**: 4K/고해상도 및 125~200% 디스플레이 배율 환경에서도 물리 픽셀 1:1 무손실 캡처.
- **반투명 마우스 드래그 영역 지정기 (`RegionSelector`)**:
  - `[✂️ 영역 선택]` 클릭 시 전체화면 반투명 오버레이 창 실행.
  - 초록색 점선 박스 및 실시간 좌표 HUD 배지 (`📍 X, Y, W, H px`) 표시 (ESC: 취소).
  - 선택 영역은 로컬 설정(`%APPDATA%\iMemo\config.json`)에 자동 저장되어 다음 실행 시 영구 유지.
  - 툴바에서 좌표 수동 미세 조정 지원.
- **macOS Swift와 100% 동일한 알고리즘 & 포맷 규격 호환**:
  - 2, 4, 8, 16컬러 실시간 팔레트 캘리브레이션.
  - 시드 `0x494D434D` XorShift32 역스크램블.
  - GF(256) 멀티블록 인터리브 Reed-Solomon 오류 정정 코덱.
  - 25% XOR 패리티 프레임 손실 자동 복원.
  - IEEE 802.3 CRC32 무결성 검증 & RFC 1951 Raw DEFLATE / Zlib 압축 해제.
- **복원 문서 활용 & 뷰어**:
  - `egui` 기반 직관적인 데스크톱 GUI.
  - 서식 미리보기(본문 텍스트 + 첨부 WebP/JPEG 이미지 인라인 렌더링) 및 HTML 소스 보기 모드.
  - **🌐 [브라우저에서 열기]**: 시스템 기본 브라우저(Edge, Chrome 등)에서 표, 제목, 서식, 첨부 이미지를 100% 원본 스타일로 즉시 열람.
  - **📋 [HTML 복사]**, **📝 [텍스트 복사]**, **💾 [HTML 저장...]** 지원.
  - 단축키: `F5`(캡처 시작/중지), `Ctrl + V`(클립보드 이미지 즉시 디코딩), `Ctrl + O`(이미지 파일 열기), `Ctrl + R`(초기화).

### 3. macOS 리더기 앱 (`ImemoReader.app`, Swift)
- **macOS 26+ (27.0.1) 호환**: Apple 최신 `ScreenCaptureKit` 기반 250ms 고속 연속 캡처.
- **원격 화면 특정 영역 캡처**:
  - **[영역 선택]**: 반투명 오버레이 창에서 마우스 드래그로 영역 지정 (실시간 좌표 HUD `X, Y, W, H` 표시 및 자동 저장).
  - 지정한 영역 좌표는 `UserDefaults`에 자동 저장되어 재사용.
  - 바코드 외곽 검정 테두리 자동 정렬 기능으로 여백이 포함되어도 정확하게 인식.
- **복원 문서 활용**:
  - `WKWebView`를 통해 서식과 이미지를 원본 그대로 렌더링.
  - 서식 미리보기 / HTML 소스 보기 전환.
  - [HTML 복사], [텍스트 복사], [.html 파일 저장] 지원.

---

## 📂 프로젝트 구조

```
imemo/
├── web/
│   └── index.html               # 클라우드 PC용 단일 파일 웹 에디터 & IMC 코드 생성기
├── ImemoReaderWin/              # Windows 네이티브 리더기 (Rust)
│   ├── Cargo.toml               # Rust 패키지 설정 파일
│   ├── README.md                # Windows 전용 상세 가이드
│   ├── src/
│   │   ├── main.rs              # 진입점 및 윈도우 뷰포트 초기화
│   │   ├── lib.rs               # 라이브러리 모듈 선언
│   │   ├── app.rs               # egui 기반 UI (서식 뷰어, 툴바, 칩스)
│   │   ├── matrix_decoder.rs    # 바코드 경계선 감지, 팔레트 보정, 셀 디코딩
│   │   ├── slide_collector.rs   # 슬라이드 프레임 수집, XOR 복구, zlib 압축해제
│   │   ├── reed_solomon.rs      # GF(256) 멀티블록 인터리브 오류정정 코덱
│   │   ├── scrambler.rs         # 비표준 키스트림 난독화 (0x494D434D)
│   │   ├── crc32.rs             # IEEE 802.3 무결성 검증
│   │   ├── screen_capture.rs    # Win32 GDI & BitBlt 화면 고속 캡처
│   │   ├── region_selector.rs   # 반투명 전체화면 마우스 드래그 오버레이 창
│   │   └── config.rs            # 캡처 좌표 자동 저장 및 로드
│   └── tests/
│       └── end_to_end_test.rs   # 2/4/16컬러 왕복 인코딩/디코딩 통합 검증
├── ImemoReader/                 # macOS 네이티브 리더기 (Swift)
│   ├── Package.swift            # Swift 패키지 설정 파일
│   ├── ImemoReader.app/         # 빌드 완료된 macOS 애플리케이션
│   └── Sources/ImemoReader/     # SwiftUI 앱 소스 코드
├── docs/
│   ├── implementation-plan.md   # 상세 아키텍처 및 구현 계획서
│   └── user-guide.md            # 사용자 운영 가이드
└── README.md                    # 본 문서
```

---

## 🚀 빠른 시작

### 1. 클라우드 PC에서 코드 생성
1. Chrome 브라우저에서 `web/index.html` 파일을 엽니다.
2. 문서를 작성하고, 필요 시 화면 캡처 이미지를 `Ctrl + V`로 붙여넣거나 `🖼️ 이미지 첨부` 버튼으로 추가합니다.
3. 설정 패널에서 가로폭(예: 1200px) 및 색상 옵션(이미지 포함 시 16컬러 권장)을 선택합니다.
4. **[이미지 생성 (IMC 코드)]** 버튼을 클릭하면 에디터 아래에 직사각형 바코드가 표시됩니다.

### 2. Windows에서 화면 캡처 및 문서 복원 (Rust)
1. `ImemoReaderWin` 디렉토리에서 실행합니다:
   ```bash
   cargo run --release
   ```
2. 상단 툴바의 **[✂️ 영역 선택]**을 클릭하고, 원격 창의 바코드 주변을 마우스로 드래그합니다 (좌표 자동 저장).
3. **[▶ 캡처 시작]** (또는 단축키 **`F5`**)를 누르면 슬라이드가 자동 수집되어 원본 서식 문서로 즉시 복원됩니다.
4. 복원된 문서는 **[HTML 복사]**, **[텍스트 복사]**, **[브라우저에서 열기]**, **[HTML 저장...]** 버튼으로 즉시 활용합니다.
5. 캡처 도구로 복사한 이미지는 리더기에서 바로 **`Ctrl + V`**를 눌러 디코딩할 수도 있습니다.

### 3. macOS에서 화면 캡처 및 문서 복원 (Swift)
1. 맥에서 `ImemoReader.app`을 실행합니다:
   ```bash
   open ImemoReader/ImemoReader.app
   ```
2. 상단 툴바의 **[영역 선택]**을 클릭하고 바코드 주변을 드래그한 뒤 **[캡처 시작]**을 누릅니다.

---

## 🛠️ 빌드 및 테스트

### Windows 리더기 (Rust)
```bash
cd ImemoReaderWin

# 단위 및 2/4/16컬러 통합 테스트 실행
cargo test

# 릴리스 바이너리 빌드
cargo build --release
# 실행 파일: target/release/imemo-reader-win.exe
```

### macOS 리더기 (Swift)
```bash
swiftc -parse-as-library -module-cache-path /tmp/swift-cache \
  ImemoReader/Sources/ImemoReader/CRC32.swift \
  ImemoReader/Sources/ImemoReader/Scrambler.swift \
  ImemoReader/Sources/ImemoReader/ReedSolomon.swift \
  ImemoReader/Sources/ImemoReader/MatrixDecoder.swift \
  ImemoReader/Sources/ImemoReader/SlideCollector.swift \
  ImemoReader/Sources/ImemoReader/RegionSelector.swift \
  ImemoReader/Sources/ImemoReader/ScreenCapturer.swift \
  ImemoReader/Sources/ImemoReader/ContentView.swift \
  ImemoReader/Sources/ImemoReader/App.swift \
  -o ImemoReader/ImemoReader.app/Contents/MacOS/ImemoReader
```

---

## 🔒 보안 및 비표준 기술 규격

| 항목 | 사양 |
|---|---|
| **코드 구조** | 가로형 비표준 매트릭스 (4코너 비대칭 파인더, 타이밍 트랙, 캘리브레이션 바) |
| **압축 규격** | RFC 1951 Raw DEFLATE (`CompressionStream('deflate-raw')` / zlib) |
| **오류 정정** | GF(256) 멀티블록 인터리브 Reed-Solomon (블록당 180B 이하, 패리티 ~30%) |
| **대용량 복구**| 순환 슬라이드 애니메이션 + 25% XOR 패리티 프레임 손실 복구 |
| **난독화** | 전용 시드(`0x494D434D`) 32비트 XorShift PRNG 키스트림 XOR |
| **무결성** | IEEE 802.3 CRC32 체크섬 (단 1비트 오염 시에도 복원 거부) |
| **색상 보정** | 실시간 상·하단 기준 팔레트 샘플링 및 Lab/RGB 유클리드 거리 분류 |

---

## 📄 라이선스
MIT License
