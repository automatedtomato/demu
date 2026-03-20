# demu — task tracking

Milestone: [Release v0.4.0](https://github.com/automatedtomato/demu/milestone/4)
Tracking issue: [#47](https://github.com/automatedtomato/demu/issues/47)

## In progress
- [x] [#48](https://github.com/automatedtomato/demu/issues/48) feat: Compose YAML parser — implementation complete, 27 new tests (18 unit + 9 integration), all 635 tests passing

## Up next
- [ ] [#49](https://github.com/automatedtomato/demu/issues/49) feat: CLI `--compose` and `--service` flags
- [ ] [#50](https://github.com/automatedtomato/demu/issues/50) feat: Compose engine — service merge
- [ ] [#51](https://github.com/automatedtomato/demu/issues/51) feat: mount shadow model
- [ ] [#52](https://github.com/automatedtomato/demu/issues/52) feat: REPL Compose commands

## Done

- [x] [#42](https://github.com/automatedtomato/demu/issues/42) feat: `COPY --from=<stage>` cross-stage file copying — merged [#45](https://github.com/automatedtomato/demu/pull/45)
- [x] [#41](https://github.com/automatedtomato/demu/issues/41) feat: multi-stage build support + `--stage` CLI flag — merged [#44](https://github.com/automatedtomato/demu/pull/44)
- [x] [#40](https://github.com/automatedtomato/demu/issues/40) feat: `:explain <path>` REPL command
- [x] [#25](https://github.com/automatedtomato/demu/issues/25) feat: RUN skipped-command warnings — merged [#36](https://github.com/automatedtomato/demu/pull/36)
- [x] [#32](https://github.com/automatedtomato/demu/issues/32) refactor: extract `io_err` closure to shared helper — merged [#37](https://github.com/automatedtomato/demu/pull/37)
- [x] [#33](https://github.com/automatedtomato/demu/issues/33) fix: sanitize env var keys/values in `env_cmd.rs` — merged [#38](https://github.com/automatedtomato/demu/pull/38)
- [x] [#24](https://github.com/automatedtomato/demu/issues/24) feat: REPL `:reload` — merged [#35](https://github.com/automatedtomato/demu/pull/35)
- [x] [#23](https://github.com/automatedtomato/demu/issues/23) feat: REPL `apt list --installed` / `pip list` — merged [#34](https://github.com/automatedtomato/demu/pull/34)
- [x] [#22](https://github.com/automatedtomato/demu/issues/22) feat: REPL `:installed` + `which` — merged [#31](https://github.com/automatedtomato/demu/pull/31)
- [x] [#21](https://github.com/automatedtomato/demu/issues/21) feat: RUN package install registry — merged [#30](https://github.com/automatedtomato/demu/pull/30)
- [x] [#20](https://github.com/automatedtomato/demu/issues/20) feat: RUN filesystem mutation simulation — merged [#29](https://github.com/automatedtomato/demu/pull/29)
- [x] [#19](https://github.com/automatedtomato/demu/issues/19) feat: RUN `&&`-chain parsing — merged [#28](https://github.com/automatedtomato/demu/pull/28)
- [x] [#26](https://github.com/automatedtomato/demu/issues/26) chore: release distribution pipeline — merged [#27](https://github.com/automatedtomato/demu/pull/27)
- [x] Milestone planning and GitHub setup
- [x] Design decisions: `tasks/decisions/001~003`
- [x] Private repo + `main` / `develop` branches
- [x] [#1](https://github.com/automatedtomato/demu/issues/1) Cargo scaffold — merged [#10](https://github.com/automatedtomato/demu/pull/10)
- [x] [#2](https://github.com/automatedtomato/demu/issues/2) feat: domain model types
- [x] [#3](https://github.com/automatedtomato/demu/issues/3) feat: Dockerfile parser (v0.1 subset)
- [x] [#4](https://github.com/automatedtomato/demu/issues/4) feat: engine — apply instructions to PreviewState
- [x] [#5](https://github.com/automatedtomato/demu/issues/5) feat: REPL — standard shell commands — merged [#14](https://github.com/automatedtomato/demu/pull/14)
- [x] [#6](https://github.com/automatedtomato/demu/issues/6) feat: REPL — custom inspection commands (`:layers`, `:history`) — merged [#15](https://github.com/automatedtomato/demu/pull/15)
- [x] [#7](https://github.com/automatedtomato/demu/issues/7) feat: CLI entrypoint — merged [#16](https://github.com/automatedtomato/demu/pull/16)
- [x] [#8](https://github.com/automatedtomato/demu/issues/8) test: integration fixtures for v0.1 — merged [#17](https://github.com/automatedtomato/demu/pull/17)
