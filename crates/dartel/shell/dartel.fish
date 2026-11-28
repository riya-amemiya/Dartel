function __dartel_prompt --on-event fish_prompt
    set -l code $status
    if set -q __dartel_running
        set -e __dartel_running
        printf '\e]133;D;%s\a' $code
    end
    if test "$PWD" != "$__dartel_directory"
        set -g __dartel_directory $PWD
        printf '\e]7;file://%s%s\a' $hostname (string escape --style=url -- $PWD)
    end
    printf '\e]133;A\a'
end

function __dartel_preexec --on-event fish_preexec
    set -g __dartel_running
    printf '\e]133;C\a'
end
