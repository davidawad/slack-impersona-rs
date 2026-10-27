{ pkgs, ... }:

{
  packages = with pkgs; [
    cargo
    rustc
    rustfmt
    clippy
    just
    pre-commit
    gitleaks
    typos
  ];

  enterShell = ''
    [ -d .git ] && [ ! -f .git/hooks/commit-msg ] && pre-commit install --install-hooks --hook-type pre-commit --hook-type commit-msg --hook-type post-commit >/dev/null 2>&1 || true
  '';
}
