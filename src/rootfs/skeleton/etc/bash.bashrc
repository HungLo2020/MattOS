# System defaults for interactive Brush/Bash shells. Per-user rc files run next.
case $- in *i*) ;; *) return ;; esac

case ${TERM:-dumb} in
    xterm*|screen*|tmux*|rxvt*|linux|konsole*)
        if [ -z "${NO_COLOR:-}" ]; then
            if [ "$(id -u)" = 0 ]; then
                PS1='\[\e[01;31m\]\u@\h\[\e[0m\]:\[\e[01;34m\]\w\[\e[0m\]\$ '
            else
                PS1='\[\e[01;32m\]\u@\h\[\e[0m\]:\[\e[01;34m\]\w\[\e[0m\]\$ '
            fi
            return
        fi
        ;;
esac
PS1='\u@\h:\w\$ '
