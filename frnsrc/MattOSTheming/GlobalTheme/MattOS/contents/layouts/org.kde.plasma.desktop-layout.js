var plasma = getApiVersion(1);

var layout = {
    "desktops": [
        {
            "applets": [],
            "config": {
                "/": {
                    "formfactor": "0",
                    "immutability": "1",
                    "wallpaperplugin": "org.kde.slideshow"
                },
                "/Wallpaper/org.kde.slideshow/General": {
                    "SlideInterval": "240",
                    "SlidePaths": "/usr/share/wallpapers/,/mnt/storage/OneDrive/Media/Wallpapers/Wide/"
                }
            },
            "wallpaperPlugin": "org.kde.slideshow"
        }
    ],
    "panels": [
        {
            "alignment": "center",
            "applets": [
                {
                    "config": {
                        "/Configuration%2FGeneral": {
                            "icon": "start-here-kde-symbolic",
                            "showAppsByName": "true"
                        },
                        "/General": {
                            "favoriteApps": "applications:org.mozilla.firefox.desktop,applications:org.kde.konsole.desktop,applications:org.kde.plasma-systemmonitor.desktop,applications:org.kde.dolphin.desktop,applications:org.kde.kate.desktop,applications:org.kde.discover.desktop",
                            "icon": "start-here-kde-symbolic",
                            "systemFavorites": "suspend\\,hibernate\\,reboot\\,shutdown"
                        },
                        "/Shortcuts": {
                            "global": "Alt+F1"
                        }
                    },
                    "plugin": "org.kde.plasma.kickoff"
                },
                {
                    "config": {},
                    "plugin": "org.kde.plasma.pager"
                },
                {
                    "config": {
                        "/General": {
                            "launchers": "applications:org.mozilla.firefox.desktop,applications:org.kde.dolphin.desktop,applications:org.kde.konsole.desktop"
                        }
                    },
                    "plugin": "org.kde.plasma.icontasks"
                },
                {
                    "config": {},
                    "plugin": "org.kde.plasma.marginsseparator"
                },
                {
                    "config": {},
                    "plugin": "org.kde.plasma.systemtray"
                },
                {
                    "config": {},
                    "plugin": "org.kde.plasma.digitalclock"
                },
                {
                    "config": {},
                    "plugin": "org.kde.plasma.minimizeall"
                }
            ],
            "height": 3.3333333333333335,
            "hiding": "normal",
            "location": "bottom",
            "maximumLength": 106.66666666666667,
            "minimumLength": 106.66666666666667,
            "offset": 0
        }
    ],
    "serializationFormatVersion": "1"
};

plasma.loadSerializedLayout(layout);
