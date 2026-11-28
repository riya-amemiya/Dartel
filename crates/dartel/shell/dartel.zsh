autoload -Uz add-zsh-hook

__dartel_precmd() {
  local code=$?
  if [[ -n $__dartel_running ]]; then
    __dartel_running=
    printf '\e]133;D;%s\a' "$code"
  fi
  if [[ $PWD != "$__dartel_directory" ]]; then
    __dartel_directory=$PWD
    __dartel_location
  fi
  printf '\e]133;A\a'
}

__dartel_preexec() {
  __dartel_running=1
  printf '\e]133;C\a'
}

__dartel_location() {
  emulate -L zsh
  setopt extendedglob nomultibyte
  printf '\e]7;file://%s%s\a' "$HOST" "${PWD//(#m)[^A-Za-z0-9\/._~-]/%${(l:2::0:)$(([##16]#MATCH))}}"
}

add-zsh-hook precmd __dartel_precmd
add-zsh-hook preexec __dartel_preexec
