# scope 수동 테스트 가이드

`Changes` 탭의 세 scope — `uncommitted`, `branch`, `last-turn` — 를 정확하게 확인하는 절차입니다.
세 scope는 각각 **기준선이 다릅니다**. 그래서 "diff가 보인다"만으로는 통과 판정이 안 되고, *무엇을
기준으로 잡았는지*까지 봐야 합니다. 아래 시나리오는 그 기준선이 틀렸을 때만 실패하도록 짜여 있습니다.

계약 원문은 `specs/review-model.md`(Scopes, Base branch, Turn baseline)와
`specs/herdr-host.md`(Turn tracking)입니다. 이 문서와 스펙이 어긋나면 스펙이 맞습니다.

## 준비

바이너리를 하나 정하고 끝까지 그것만 씁니다. 릴리스 빌드가 실사용에 가깝습니다.

```
cargo build --release
```

Windows: `target\release\herdr-reviewr.exe` · macOS/Linux: `target/release/herdr-reviewr`

### CLI 인자

| 인자 | 뜻 |
| --- | --- |
| `<경로>` | 리뷰할 저장소. 생략하면 현재 디렉터리 |
| `--base <ref>` | `branch` scope의 base를 지정. 우선순위 1위라 picker와 `origin/HEAD`를 모두 제칩니다 |
| `--poll <ms>` | 폴링 간격. 하한 200ms. `last-turn` 테스트에서 반응을 빠르게 볼 때 유용 |
| `--theme <이름>` / `--wrap on\|off` | 표시 옵션 |

### 로그 켜기

판정이 애매할 때 로그가 유일한 근거입니다. 테스트 내내 켜두는 걸 권합니다.

```powershell
$env:HERDR_REVIEW_LOG = "$env:TEMP\reviewr-qa.log"    # PowerShell
```
```bash
export HERDR_REVIEW_LOG=/tmp/reviewr-qa.log            # bash
```

### 키

| 키 | 동작 |
| --- | --- |
| `u` / `b` / `t` | scope를 uncommitted / branch / last-turn 으로 전환 |
| `1` / `2` / `3` | 탭을 Changes / All files / PR 로 전환 |
| `B` | base picker 열기 |
| `r` | 즉시 새로고침 |
| `q` | 종료 |

헤더의 scope chip을 클릭하면 세 scope를 순환합니다. `branch` scope일 때는 헤더가 base 이름을 같이
보여주므로, **무엇과 비교 중인지 화면에서 직접 확인**할 수 있습니다.

## 테스트용 저장소 만들기

세 scope를 한 저장소에서 모두 구분하려면, 커밋된 변경과 커밋 안 된 변경이 같이 있어야 합니다.
아래 스크립트가 그 상태를 만듭니다. bash(Git Bash 포함)에서 실행합니다.

```bash
set -e
cd "$(mktemp -d)"                      # Windows Git Bash도 동작
git init -q -b main reviewr-qa && cd reviewr-qa   # -b main 필수: 기본 브랜치가 master인 환경이 있음
git config user.email qa@example.com && git config user.name QA

# base 브랜치가 될 main
printf 'base line\n' > base.txt
printf 'ignored\n'   > .gitignore
git add -A && git commit -qm "base"

# 작업 브랜치: 커밋된 변경 1개
git switch -qc feature
printf 'committed change\n' > committed.txt
git add -A && git commit -qm "committed work"

# 커밋 안 한 변경 3종
printf 'staged change\n'   > staged.txt   && git add staged.txt   # staged
printf 'base line\nunstaged change\n' > base.txt                  # unstaged
printf 'untracked\n'       > untracked.txt                        # untracked
printf 'hidden\n'          > ignored                              # .gitignore 대상

echo "만든 곳: $PWD"
```

이 상태에서 각 scope가 보여야 할 것:

| scope | 보여야 함 | 보이면 안 됨 |
| --- | --- | --- |
| `uncommitted` | `staged.txt`, `base.txt`, `untracked.txt` | `committed.txt`, `ignored` |
| `branch` | 위 셋 + `committed.txt` | `ignored` |
| `last-turn` | (아래 별도 절차) | — |

## scope 1 — `uncommitted`

**기준선: `HEAD`.** staged + unstaged + untracked를 봅니다.

1. 위 저장소에서 실행: `herdr-reviewr.exe .`
2. 헤더의 scope chip이 `uncommitted`인지 확인합니다. 설정에 `default_scope`가 없으면 여기서
   시작하므로, **이 상태에서 `u`를 눌러도 아무 변화가 없는 게 정상입니다.** `u`가 실제로 동작하는지
   보려면 `b`로 옮겼다가 `u`로 돌아와야 합니다.
3. 파일 목록에 `staged.txt`, `base.txt`, `untracked.txt` 세 개가 있는지 확인.
4. `committed.txt`가 **없는지** 확인. 있으면 기준선이 `HEAD`가 아니라 base를 잡은 것입니다.
5. `ignored`가 **없는지** 확인.
6. `2`로 `All files` 탭에 가면 `ignored`가 흐리게 표시되어 나타납니다. 여기 있는 게 맞습니다 —
   `.gitignore` 제외는 `Changes`에만 걸립니다.
7. `untracked.txt`를 열면 전체 줄이 추가로 표시되는지 확인. 추적되지 않은 파일은 전부가 추가입니다.

**함정.** untracked 파일이 안 보인다면 `.gitignore`에 걸린 건 아닌지 먼저 보세요. `ignored`라는
이름이 `.gitignore`에 있으므로, 새 파일을 만들 때 그 이름을 피해야 합니다.

## scope 2 — `branch`

**기준선: base와 `HEAD`의 merge-base.** 브랜치가 base 위에 쌓은 전부를 봅니다.

### 2-1. 기본 확인

1. `--base`로 base를 못 박고 실행합니다. 이러면 picker나 `origin/HEAD` 상태와 무관하게 결과가 정해집니다.
   ```
   herdr-reviewr.exe --base main .
   ```
2. `b`를 눌러 scope를 `branch`로.
3. 헤더가 base 이름으로 `main`을 보여주는지 확인.
4. `uncommitted`의 세 파일 **더하기** `committed.txt`가 보이는지 확인. 커밋 여부와 무관하게
   브랜치가 가진 전부가 나와야 합니다.

### 2-2. merge-base인지 확인 (가장 중요)

단순히 `main`의 현재 끝과 비교한다면 남의 커밋이 섞여 들어옵니다. 이걸 구분하는 시나리오입니다.

```bash
git stash -u -q                    # 미커밋 변경을 잠시 치워둡니다
git switch -q main
printf 'someone else\n' > other.txt
git add other.txt                  # -A 금지: 작업 트리 전부를 main으로 쓸어담습니다
git commit -qm "unrelated work on main"
git switch -q feature
git stash pop -q                   # 미커밋 변경 복구
```

`git stash`로 감싸는 게 핵심입니다. 그냥 브랜치를 옮기면 미커밋 변경이 따라가고, 거기서 `git add -A`를
하면 그 변경들이 `main`에 커밋되어 테스트 상태가 통째로 무너집니다.

이제 `main`은 `feature`가 갈라진 뒤로 앞서 나갔습니다. 다시 실행해서 `b`를 누릅니다.

- **통과:** `other.txt`가 보이지 **않습니다**. merge-base(갈라진 지점)와 비교하고 있다는 뜻입니다.
- **실패:** `other.txt`가 삭제로 표시됩니다. `main`의 현재 끝과 비교한 것입니다.

### 2-3. base 해석 순서

`--base`가 1순위, picker 선택이 2순위, `origin/HEAD`가 3순위입니다.

1. `--base` 없이 실행하고, **먼저 `b`를 눌러 `branch` scope로 들어갑니다.** 이 저장소는 remote가
   없어 `origin/HEAD`도 없으므로, 헤더가 `[branch] no base`를 보여주고 푸터가 base picker를
   권해야 합니다. **이건 에러가 아니라 정상 동작입니다.**
2. 이제 `B`(Shift+b)를 눌러 picker를 열고 `main`을 고릅니다. 곧바로 changeset이 다시 만들어져야
   합니다 — 폴링을 기다리지 않습니다.

   > **`B`가 안 먹는 것처럼 보인다면 정상입니다.** base picker는 `branch` scope가 활성일 때만,
   > 그리고 `--base` 플래그를 주지 않았을 때만 동작합니다. 그 밖에서는 무반응이고 푸터에도
   > 나타나지 않습니다 (`specs/input.md` Base picker). `uncommitted` 상태에서 `B`를 누르면
   > 아무 일도 안 일어나는 게 계약대로입니다.

3. `q`로 끄고 다시 켭니다. 고른 base가 유지되는지 확인. 선택은 `refs/reviewr/base-pick`이라는
   private ref에 저장되므로 재시작을 넘어갑니다. 저장된 이름을 직접 읽는 방법:
   ```
   git cat-file blob refs/reviewr/base-pick
   ```
   고른 브랜치 이름이 그대로 나옵니다. reviewr 자신도 정확히 이 명령으로 읽으므로
   (`src/git.rs`의 `read_base_pick`), 가장 믿을 만한 확인입니다.

   > ref 목록을 보고 싶으면 `git for-each-ref refs/reviewr/`입니다. 인자는 **파일 경로가 아니라
   > ref 이름 패턴**이라 `.git/`을 붙이지 않습니다. 출력이 비었다면 대개 저장소를 잘못 짚은
   > 것입니다 — 반드시 테스트 저장소 안에서 실행하세요. `ls .git/refs/reviewr/`는 같은 것을
   > 파일시스템 쪽에서 보는 방법이고, 이때는 `.git/` 경로가 맞습니다.

4. 이제 `--base main`을 주고 켜면, picker 선택과 무관하게 `--base`가 이깁니다. 이 상태에서는
   `B`도 무반응이 정상입니다(위 상자 참고).

**존재하지 않는 base:** `--base no-such-branch`로 켜면 그 소스는 건너뛰어지고 다음 순위로 넘어갑니다.
에러로 죽으면 안 됩니다. 그리고 `uncommitted`와 `last-turn`은 영향을 받지 않아야 합니다.

## scope 3 — `last-turn`

**기준선: 파일을 실제로 바꾼 가장 최근 턴이 시작되던 순간의 워크트리.**

여기가 다른 둘과 결정적으로 다릅니다. 이 기준선은 git 이력이 아니라 **herdr가 보고하는 에이전트
상태 변화**에서 나옵니다. 그래서 `herdr` CLI 없이 맨몸으로 `cargo run`만 하면 이 scope는
**비어 있는 게 정상**입니다. 이걸 결함으로 오해하지 마세요.

reviewr가 턴 시작을 아직 한 번도 관측하지 못했으면 기준선이 없고, 화면은 "비었다"가 아니라
"기다리는 중"으로 보고합니다.

**필요한 것은 herdr CLI뿐이며, 플러그인 페인이 아닙니다.** 턴 추적이 하는 일은 `herdr agent list`를
서브프로세스로 부르는 것뿐입니다(`src/herdr.rs`). 플러그인 페인의 toggle/open/close 액션은 별개이고
그쪽이 Phase 2입니다. herdr 0.8.0은 Windows에서 CLI 전체가 동작하고 `agent list`가 `cwd`를 native
형식(`D:\workspace\...`)으로 실어 보내므로, **세 플랫폼 모두에서 아래 절차가 그대로 통합니다.**

### 3-1. 실제 herdr 에이전트로 (권장)

가장 진짜에 가까운 경로입니다. 핵심은 **에이전트와 reviewr가 같은 워크트리를 봐야 한다**는 것입니다.
에이전트의 `cwd`가 reviewr가 리뷰 중인 저장소의 git top level로 해석되어야 "이 워크트리 안에 있다"고
판정됩니다.

1. herdr 페인에서 에이전트(예: claude)를 워크트리 `X`에서 돌립니다.
2. **다른** 터미널에서 reviewr를 같은 워크트리에 붙여 띄웁니다. 폴링을 빠르게 두면 관찰이 쉽습니다.
   ```
   herdr-reviewr.exe --poll 200 X
   ```
   에이전트가 잡히는지는 이 명령으로 미리 확인할 수 있습니다 — `cwd`가 `X`인 항목이 있어야 합니다.
   ```
   herdr agent list
   ```
3. 에이전트에게 파일을 바꾸는 작업을 시킵니다.
4. `t`를 눌러 `last-turn`으로. 작업 **중에는 진행 중인 턴**이 보입니다.
5. 에이전트가 끝나면 방금 끝난 턴이 보입니다.
6. 다음 네 가지를 확인합니다.

| 확인할 것 | 기대 동작 |
| --- | --- |
| 파일을 안 바꾼 턴 (질문만 하고 끝) | 기준선이 그대로. 직전 작업 결과가 계속 보임 |
| 턴 도중 `git commit` | 기준선이 안 움직임. diff가 사라지지 않음 |
| 리뷰어가 직접 편집 | 에이전트 변경과 같은 diff에 섞여 나옴 (턴 단위이지 작성자 단위가 아님) |
| 권한 프롬프트로 `blocked` | 같은 턴이 이어짐. 새 턴이 시작되지 않음 |

기준선 ref는 이렇게 직접 볼 수 있습니다:

```
git for-each-ref refs/reviewr/turn-base/
```

### 3-2. 가짜 herdr로 (herdr 없이)

`HERDR_BIN_PATH`가 herdr 실행 파일을 갈아끼웁니다. 이걸로 에이전트 상태를 직접 조종해서 턴 경계를
만들 수 있습니다. reviewr가 읽는 것은 `herdr agent list`의 stdout뿐입니다.

기대하는 형식은 이렇습니다. `cwd`가 리뷰 중인 워크트리의 git top level로 해석되어야 그 에이전트가
"이 워크트리 안에 있다"고 판정됩니다.

```json
{"result":{"agents":[{"agent":"claude","agent_status":"working","pane_id":"w1:p1","tab_id":"w1:t1","workspace_id":"w1","cwd":"<워크트리 절대경로>"}]}}
```

`agent_status`가 읽히는 철자는 넷입니다: `idle`, `working`, `blocked`, `done`. 그 외 문자열은
모두 `unknown`으로 접히고, `unknown`은 쉬는 상태가 **아닙니다**.

- **턴 시작** = 쉼 → 일함. 즉 `idle` → `working`.
- **턴 종료** = 다시 쉼. 즉 `working` → `idle`.
- **첫 표본은 턴을 시작시키지 않습니다.** 상태 변화의 *가장자리*를 보는 것이므로, 처음부터
  `working`으로 시작하면 아무 일도 일어나지 않습니다. 반드시 `idle`로 시작해서 `working`으로
  바꿔야 합니다.

절차:

1. 상태를 파일에서 읽어 출력하는 가짜 herdr를 만듭니다.

   **Windows (`.cmd`)** — `cwd`는 반드시 슬래시로 씁니다. JSON 문자열 안의 역슬래시는 이스케이프가
   필요한데, 슬래시로 쓰면 그 문제가 사라지고 git과 `Path::is_absolute` 모두 정상 인식합니다.
   ```bat
   @echo off
   if not "%~1"=="agent" exit /b 0
   if not "%~2"=="list" exit /b 0
   set /p ST=<"%QA_STATUS%"
   echo {"result":{"agents":[{"agent":"claude","agent_status":"%ST%","pane_id":"w1:p1","tab_id":"w1:t1","workspace_id":"w1","cwd":"%QA_WORKTREE%"}]}}
   ```
   PowerShell에서 환경을 세팅합니다. `QA_STATUS` 파일은 개행 없이 써야 `set /p`가 깨끗하게 읽습니다.
   ```powershell
   $env:QA_STATUS     = "$PWD\qa-status.txt"
   $env:QA_WORKTREE   = "C:/절대/경로/reviewr-qa"     # 슬래시
   $env:HERDR_BIN_PATH = "$PWD\fake-herdr.cmd"
   "idle" | Set-Content -NoNewline $env:QA_STATUS
   ```
   상태를 바꿀 때도 같은 방식입니다: `"working" | Set-Content -NoNewline $env:QA_STATUS`

   **macOS / Linux (`sh`)**
   ```bash
   cat > /tmp/fake-herdr <<'EOF'
   #!/bin/sh
   [ "$1" = "agent" ] && [ "$2" = "list" ] || exit 0
   printf '{"result":{"agents":[{"agent":"claude","agent_status":"%s","pane_id":"w1:p1","tab_id":"w1:t1","workspace_id":"w1","cwd":"%s"}]}}\n' \
     "$(cat /tmp/qa-status)" "$QA_WORKTREE"
   EOF
   chmod +x /tmp/fake-herdr
   echo idle > /tmp/qa-status
   export QA_WORKTREE=/절대/경로/reviewr-qa
   export HERDR_BIN_PATH=/tmp/fake-herdr
   ```
   만들었으면 먼저 단독으로 불러서 형식을 확인합니다. 위 JSON 한 줄이 그대로 나와야 합니다.
   ```
   fake-herdr.cmd agent list        # 또는  /tmp/fake-herdr agent list
   ```
2. `--poll 200`으로 reviewr를 띄우고 `t`를 누릅니다. 아직 턴을 못 봤으니 비어 있어야 합니다.
3. 상태를 `working`으로 바꿉니다 — 턴이 시작됩니다. 이 순간의 워크트리가 후보 기준선입니다.
4. 파일을 하나 고칩니다. 그 변경이 `last-turn`에 나타나야 합니다. 후보가 실제 기준선으로 승격되는
   건 **파일이 바뀐 것을 본 첫 폴링**에서입니다.
5. 상태를 `idle`로 되돌립니다 — 턴이 끝납니다. 방금 끝난 턴이 계속 보여야 합니다.
6. **파일 안 바꾸는 턴:** `working` → `idle`을 파일 수정 없이 반복합니다. 3~4단계의 diff가
   그대로 남아 있어야 합니다. 사라지면 기준선이 잘못 움직인 것입니다.
7. **blocked가 턴을 안 쪼개는지:** `working` → `blocked` → `working`. 하나의 턴으로 이어져야
   하고, 3단계 이후의 변경이 전부 남아 있어야 합니다.

**진행이 없을 때 볼 곳.** `HERDR_REVIEW_LOG`를 켜두면 herdr 호출이 실패했을 때만 로그가 남습니다
(성공한 호출은 조용합니다). 로그에 아무것도 없는데 턴도 안 잡힌다면 대개 둘 중 하나입니다:
첫 표본이 이미 `working`이었거나(2번 항목 참고), `cwd`가 리뷰 중인 워크트리로 해석되지 않는 경우입니다.
`herdr agent list`를 직접 불러 `cwd`를 눈으로 확인하세요.

## 부록 — 브라우저 열기 (플랫폼 오프너)

scope와는 별개지만, 플랫폼 오프너는 사람이 눈으로 봐야만 확인되는 유일한 항목이라 여기 함께 둡니다.
Windows에서는 셸의 프로토콜 연결(`ShellExecuteW`)을 직접 호출합니다. 자동 테스트로는 브라우저 창이
떴는지 볼 방법이 없으므로, 이 절차가 유일한 검증입니다.

**실패는 조용합니다.** 여기서 실제로 겪은 결함이 그랬습니다: `explorer.exe`로 열던 시절, 쿼리
스트링이 붙은 URL은 브라우저 대신 **탐색기 폴더 창**을 열었고 종료 코드는 성공을 보고했습니다.
그래서 "에러가 안 났다"는 통과 근거가 되지 못합니다. 반드시 무엇이 떴는지 눈으로 보세요.

포지 계정이나 PR 없이도 확인할 수 있습니다. 렌더된 마크다운의 링크가 같은 오프너를 씁니다.

1. 테스트 저장소에 링크가 든 마크다운 파일을 만듭니다. 셸이 재해석할 바이트를 일부러 섞습니다.
   ```bash
   printf '# link test\n\n[plain](https://example.com/)\n\n[hostile](https://example.com/a%%20b?x=1&y=2)\n' > link.md
   ```
2. reviewr를 띄우고 `2`로 `All files` 탭에 갑니다. `link.md`를 엽니다.
3. `m`을 눌러 마크다운 미리보기로 전환합니다. 링크가 강조 텍스트 + 흐린 목적지로 보여야 합니다.
4. 링크를 **마우스로 클릭**합니다. 키보드로는 열 수 없습니다 (`specs/markdown.md` Non-goals).
5. **기본 브라우저**가 뜨는지 봅니다. 푸터에도 `opened link in browser`가 표시됩니다.
   탐색기 창이 떴다면 실패입니다.
6. 두 번째 링크가 핵심입니다. 쿼리 스트링이 붙어 있어서, 예전에 깨지던 바로 그 모양입니다.
   주소창의 URL이 `https://example.com/a%20b?x=1&y=2`와 **정확히** 같아야 합니다. `%20`이
   사라지거나 `&y=2`가 잘려 있으면 명령 프로세서를 거친 것입니다.

PR이 있는 저장소라면 `3`으로 `PR` 탭에 가서 `o`를 눌러도 같은 경로를 지납니다. 이쪽은 `gh` 같은
포지 CLI 인증이 필요합니다.

## 세 scope 공통 확인

1. **`.gitignore` 존중.** 세 scope 모두에서 `ignored`가 `Changes`에 없어야 합니다.
   `All files`에는 흐리게 있어야 합니다.
2. **scope 전환이 상태를 안 깨는지.** `u` → `b` → `t` → `u`로 한 바퀴 돌립니다. 파일 목록의 커서,
   스크롤, 접힘이 사용자 입력에 의해서만 움직여야 합니다.
3. **코멘트가 살아남는지.** `uncommitted`에서 코멘트를 하나 달고(`c`, 입력, `enter`),
   scope를 바꿨다가 돌아옵니다. 코멘트가 그대로 있어야 합니다. `r`로 새로고침해도 마찬가지입니다.
   코멘트는 명시적인 export로만 사라집니다.
4. **쓰기가 없는지.** 테스트를 마친 뒤 저장소 상태를 확인합니다. reviewr는 워크트리·인덱스·브랜치를
   절대 건드리지 않습니다. 유일한 git 쓰기는 `refs/reviewr/` 아래 private ref 둘(턴 기준선, base 선택)입니다.
   ```bash
   git status --short          # reviewr 실행 전후가 같아야 함
   git for-each-ref refs/reviewr/   # 여기에만 흔적이 남음
   ```

## 통과 판정

| # | 항목 | 통과 조건 |
| --- | --- | --- |
| 1 | `uncommitted` | staged·unstaged·untracked만. 커밋된 것과 ignore된 것은 없음 |
| 2 | `branch` 기본 | 위 셋 + 커밋된 변경 |
| 3 | `branch` merge-base | base가 앞서 나가도 남의 커밋이 안 섞임 |
| 4 | `branch` base 순서 | `--base` > picker > `origin/HEAD`. 해석 실패는 건너뜀이지 에러가 아님 |
| 5 | base 선택 보존 | 재시작 후에도 유지 |
| 6 | `last-turn` 미관측 | 비어 있고, "기다리는 중"으로 보고 |
| 7 | `last-turn` 턴 경계 | 시작은 쉼→일함에서만, 종료는 다시 쉼에서 |
| 8 | `last-turn` 무변경 턴 | 기준선이 안 움직임 |
| 9 | `last-turn` 커밋 | 기준선이 안 움직임 |
| 10 | `.gitignore` | 세 scope 모두 `Changes`에서 제외, `All files`에는 흐리게 |
| 11 | 코멘트 보존 | scope 전환과 새로고침을 넘어 살아남음 |
| 12 | 쓰기 없음 | `refs/reviewr/` 밖으로는 아무것도 안 바뀜 |
| 13 | 브라우저 열기 | 기본 브라우저가 뜨고, `%20`과 `&`가 그대로인 URL이 열림 |

3-2(가짜 herdr)는 3-1의 대체 수단입니다. 실제 herdr 에이전트로 6~9번을 확인했다면 건너뛰어도 됩니다.
herdr를 붙일 수 없는 환경이거나, 턴 경계를 손으로 하나씩 밟아보고 싶을 때만 쓰세요.

## 관련 문서

- `specs/review-model.md` — Scopes, Base branch, Turn baseline
- `specs/herdr-host.md` — Turn tracking
- `specs/input.md` — 키맵
- `docs/qa-install.md` — 실제 herdr 페인에 로컬 빌드 넣기
