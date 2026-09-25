var plasma = getApiVersion(1);

var layout = {
    "desktops": [{
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
    }],
    "panels": [{
        "alignment": "center",
        "applets": [
            {
                "config": {
                    "/Configuration%2FGeneral": {
                        "icon": "start-here-kde-symbolic",
                        "showAppsByName": "true"
                    },
                    "/General": {
                        "favoriteApps": "applications:org.mozilla.firefox.desktop,applications:org.kde.konsole.desktop,applications:org.kde.plasma-systemmonitor.desktop,applications:org.kde.dolphin.desktop,applications:org.kde.kate.desktop",
                        "icon": "start-here-kde-symbolic",
                        "systemFavorites": "suspend\\,hibernate\\,reboot\\,shutdown"
                    }
                },
                "plugin": "org.kde.plasma.kickoff"
            },
            { "config": {}, "plugin": "org.kde.plasma.pager" },
            {
                "config": {
                    "/General": {
                        "launchers": "applications:org.mozilla.firefox.desktop,applications:org.kde.dolphin.desktop,applications:org.kde.konsole.desktop"
                    }
                },
                "plugin": "org.kde.plasma.icontasks"
            },
            { "config": {}, "plugin": "org.kde.plasma.marginsseparator" },
            { "config": {}, "plugin": "org.kde.plasma.systemtray" },
            { "config": {}, "plugin": "org.kde.plasma.digitalclock" },
            { "config": {}, "plugin": "org.kde.plasma.minimizeall" }
        ],
        "height": 3.3333333333333335,
        "hiding": "normal",
        "location": "bottom",
        "maximumLength": 106.66666666666667,
        "minimumLength": 106.66666666666667,
        "offset": 0
    }],
    "serializationFormatVersion": "1"
};

plasma.loadSerializedLayout(layout);

// The package builder renders these values from panel-defaults.conf. Plasma's
// layout API uses physical pixels for height and strings for the panel modes.
var mattosPanels = plasma.panels();
if (mattosPanels.length !== 1) {
    throw new Error("MattOS default layout expected exactly one panel");
}
var mattosPanel = mattosPanels[0];
mattosPanel.floating = @@MATTOS_PANEL_FLOATING@@;
mattosPanel.lengthMode = "@@MATTOS_PANEL_LENGTH_MODE@@";
mattosPanel.opacity = "@@MATTOS_PANEL_OPACITY@@";
mattosPanel.hiding = "@@MATTOS_PANEL_HIDING@@";
mattosPanel.height = @@MATTOS_PANEL_THICKNESS@@;
