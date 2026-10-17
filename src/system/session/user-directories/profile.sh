# Initialize standard folders for ordinary interactive login accounts.
# Graphical sessions also use upstream's early systemd user service.
case $- in
    *i*)
        if [ -n "${HOME:-}" ] && [ -x /usr/bin/xdg-user-dirs-update ] &&
           [ "$(id -u)" -ge 1000 ]; then
            /usr/bin/xdg-user-dirs-update || :
        fi
        ;;
esac
