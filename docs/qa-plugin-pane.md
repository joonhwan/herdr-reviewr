# 플러그인 pane 수동 테스트 가이드 (Windows)

herdr 플러그인으로서의 `toggle` / `open` / `close` 액션과 `worktree.created` 자동 열기를 확인하는
절차입니다. `tests/pane_actions.rs`가 이제 Windows에서도 돌지만, 거기서 herdr는 가짜입니다 — 호스트가
프로세스 트리를 어떻게 보고하느냐, pane이 실제로 어디에 열리느냐는 여기서만 확인됩니다.

**pane을 여는 것은 사람의 키스트로크입니다.** 이 문서의 열기 단계는 전부 직접 누르셔야 합니다.
스크립트로 열면 `HERDR_WORKSPACE_ID`를 무시하고 지금 보고 있는 workspace에 pane이 쌓입니다.

계약 원문은 `specs/herdr-host.md`(Pane identity, Pane actions, Pane placement, Install paths)입니다.
이 문서와 스펙이 어긋나면 스펙이 맞습니다.

## 준비

### 1. `sh` 확인

Windows에서 액션은 `sh`를 거쳐 `bash`에 닿습니다. 이게 없으면 액션이 아무 일도 못 합니다.

```
where sh
```

경로가 안 나오면 Git for Windows의 `bin` 디렉터리(`sh.exe`가 있는 곳)를 `PATH`에 넣어야 합니다.
기본 설치는 `cmd` 디렉터리만 올리는데 거기엔 `sh.exe`가 없습니다.

`PATH`의 `bash`는 확인할 필요가 없습니다. Windows에서 그 이름은 `C:\Windows\system32\bash.exe`,
즉 WSL 런처로 갑니다 — 그래서 매니페스트가 `bash`를 직접 부르지 않습니다.

### 2. 바이너리 배치

`herdr plugin link`는 빌드 단계를 건너뛰므로 `bin/`을 직접 채워야 합니다.

```
cargo build --release
mkdir -p bin
cp target/release/herdr-reviewr.exe bin/herdr-reviewr.exe
```

`bin/`은 `.gitignore`에 있습니다.

### 3. 플러그인 링크

```
herdr plugin link .
herdr plugin action list --plugin persiyanov.reviewr
```

세 액션이 나오고, 각 커맨드가 `["sh","-c","exec bash herdr/pane.sh …"]` 형태여야 합니다.

### 4. 키 바인딩

herdr 사용자 설정에 넣습니다. 매니페스트가 아니라 herdr 쪽 설정입니다.

```toml
[[keys.command]]
key = "ctrl+r"
type = "plugin_action"
command = "persiyanov.reviewr.toggle"
```

`command`는 `<plugin_id>.<action_id>`이고, `plugin_id`는 매니페스트의 `id`(`persiyanov.reviewr`)
입니다. `name`이 아닙니다.

### 5. 로그 보는 법

액션은 화면에 아무것도 안 띄웁니다. 결과는 전부 여기 남습니다.

```
herdr plugin log list --plugin persiyanov.reviewr
```

각 항목의 `status`, `exit_code`, `stdout`, `stderr`를 봅니다. 성공은 stdout 한 줄, 거부는
stderr 한 줄에 exit 1입니다.

## 테스트

git 저장소인 workspace에서 시작합니다. 저장소가 아니면 열기가 거부되는 게 정상입니다.

### T1 — 토글로 열기

기대: 오른쪽에 split으로 reviewr pane이 열리고, 포커스가 그쪽으로 갑니다.

1. 바인딩한 키를 누릅니다. 
2. pane이 열렸는지 봅니다. 
3. 로그 확인: `opened <pane_id> (split) in <workspace>`, exit 0.

**여기서 실패하면** stderr를 먼저 봅니다. `no such file or directory`가 뜨면 준비 2번(`bin/`)이,
아무 로그도 없으면 준비 1번(`sh`)이 문제입니다.

### T2 — 토글로 닫기 (가장 중요)

기대: **같은 pane이 닫힙니다.** 두 번째 pane이 쌓이면 안 됩니다.

1. T1 상태에서 같은 키를 다시 누릅니다.
2. pane이 사라졌는지 봅니다.
3. 로그: `closed <pane_id> in <workspace>`, exit 0.

이게 pane 식별(Pane identity)을 검사하는 핵심 항목입니다. Windows에서 `argv0`은
`C:\…\herdr-reviewr.exe`처럼 역슬래시 전체 경로로 오는데, 이걸 못 알아보면 "열린 pane이 없다"고
판단해서 **닫는 대신 하나 더 엽니다.** pane이 두 개가 되면 실패입니다.

### T3 — 열기는 멱등

기대: 이미 열려 있으면 아무 일도 안 합니다.

1. 키로 pane을 엽니다.
2. `herdr plugin action invoke open --plugin persiyanov.reviewr`
3. 로그: `open: already open (<pane_id>) in <workspace>`, exit 0. pane 개수는 그대로.

### T4 — 닫기는 수렴

기대: 없는 걸 닫아도 에러가 아닙니다.

1. pane이 없는 상태에서
   `herdr plugin action invoke close --plugin persiyanov.reviewr`
2. 로그: `close: nothing open in <workspace>`, exit 0.
3. 한 번 더 불러도 같은 결과.

`close`와 `open`은 스크립트로 불러도 됩니다. 여는 것만 키스트로크입니다.

### T5 — 저장소 밖에서 거부

기대: git 저장소가 아닌 곳에서는 열기가 거부됩니다.

1. 저장소가 아닌 디렉터리에서 workspace를 하나 엽니다.
2. 토글 키를 누릅니다.
3. 로그: exit 1, stderr에 `not a git repo: '<경로>'` 한 줄. pane은 안 열립니다.

경로가 `<no cwd>`로 나오면 cwd 폴백이 깨진 것입니다 — Windows에서 `pane list`에
`foreground_cwd`가 없으므로 `HERDR_PLUGIN_CONTEXT_JSON`의 `focused_pane_cwd`를 씁니다.

### T6 — 새 worktree 자동 열기

기대: worktree를 만들면 reviewr pane이 자동으로 열리되, **포커스는 안 뺏깁니다.**

1. 설정의 `auto_open`이 `true`인지 확인합니다 (기본값).
2. herdr로 worktree를 새로 만듭니다.
3. 새 workspace에 reviewr pane이 있는지 봅니다.
4. 포커스가 원래 있던 곳에 그대로인지 봅니다.

`auto_open = false`로 바꾸고 다시 해보면 pane이 안 열려야 합니다.

### T7 — 배치 설정

`toggle_placement`를 바꿔가며 T1을 반복합니다.

| 값        | 기대                                      |
| --------- | ----------------------------------------- |
| `split`   | 분할, `toggle_direction` 방향             |
| `tab`     | 새 탭, 탭 이름이 `reviewr`                |
| `zoomed`  | 확대된 pane                               |
| `overlay` | 오버레이                                  |

### T8 — 낡은 바이너리 사본이 안 남는지

기대: Windows는 `~/.local` 아래 고정 실행 경로를 **안 만듭니다.**

1. 액션을 몇 번 실행한 뒤 확인합니다.

```
ls ~/.local/state/herdr/plugins/persiyanov.reviewr/bin/ 2>/dev/null
ls ~/.local/bin/herdr-reviewr* 2>/dev/null
```

2. 둘 다 없어야 합니다.

Windows에서 심볼릭 링크는 개발자 모드나 권한 상승이 필요하고, MSYS는 조용히 **복사**로 넘어갑니다.
복사본이 생기면 플러그인을 지운 뒤에도 낡은 빌드가 계속 실행됩니다 — 이 경로가 애초에 막으려던
바로 그 상황입니다. 그래서 Windows에서는 아예 안 만듭니다.

## 통과 판정

T1~T8이 모두 기대대로면 통과입니다. 그중 **T2가 하나라도 어긋나면 나머지가 다 맞아도 실패**로
봅니다 — pane이 쌓이는 것은 사용자가 매번 손으로 치워야 하는 증상이고, 식별이 깨졌다는 신호입니다.

## 뒷정리

```
herdr plugin unlink persiyanov.reviewr
```

## 관련 문서

- `specs/herdr-host.md` — 계약 원문
- `docs/qa-scopes.md` — `Changes` 탭 세 scope의 수동 테스트
- `docs/plans/2026-08-14-windows-support/plan.md` — Windows 이식 계획과 측정 기록
