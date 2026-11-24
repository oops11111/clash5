#!/bin/bash
/usr/bin/clash5-service-uninstall

. /etc/os-release

if [ "$ID" = "deepin" ]; then
    if [ -f "/usr/share/applications/clash5.desktop" ]; then
        echo "Removing deepin desktop file"
        rm -vf "/usr/share/applications/clash5.desktop"
    fi
fi

