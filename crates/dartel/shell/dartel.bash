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

__dartel_location() {
  local LC_ALL=C
  local encoded= character index
  for (( index = 0; index < ${#PWD}; index++ )); do
    character=${PWD:index:1}
    case $character in
      [A-Za-z0-9/._~-]) encoded+=$character ;;
      *) printf -v character '%%%02X' "'$character"; encoded+=$character ;;
    esac
  done
  printf '\e]7;file://%s%s\a' "$HOSTNAME" "$encoded"
}

if [[ $PROMPT_COMMAND != *__dartel_precmd* ]]; then
  PROMPT_COMMAND=__dartel_precmd${PROMPT_COMMAND:+$'\n'$PROMPT_COMMAND}
fi

if [[ $PS0 != *__dartel_running* ]]; then
  PS0='${PS0:0:(__dartel_running=1)*0}\e]133;C\a'$PS0
fi
