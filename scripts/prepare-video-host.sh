#!/usr/bin/env bash
# Prepare the two existing ec2admin.stackworks.net services for their next
# restart. This stages environment only; it never restarts or deploys a service.
set -euo pipefail

if [[ ${EUID} != 0 ]]; then
    echo "Run as root on ec2admin.stackworks.net." >&2
    exit 1
fi
if [[ $(hostname -f) != ec2admin.stackworks.net ]]; then
    echo "This script is specific to ec2admin.stackworks.net." >&2
    exit 1
fi

for tool in /usr/bin/ffmpeg /usr/bin/ffprobe; do
    if [[ ! -x $tool ]]; then
        echo "Missing $tool; install the Ubuntu ffmpeg package first." >&2
        exit 1
    fi
done

scratch_root=/mnt/tenrankai-video
install -d -m 0755 "$scratch_root"

for entry in tenrankai.service:main tenrankai-dot-com.service:docs; do
    service=${entry%%:*}
    directory="$scratch_root/${entry##*:}"
    service_user=$(systemctl show "$service" --property=User --value)
    service_group=$(systemctl show "$service" --property=Group --value)
    if [[ -z $service_user || $service_user == root ]]; then
        echo "Expected an existing unprivileged service user for $service." >&2
        exit 1
    fi
    if [[ -z $service_group ]]; then
        service_group=$(id -gn "$service_user")
    fi
    install -d -o "$service_user" -g "$service_group" -m 0700 "$directory"
    drop_in_directory="/etc/systemd/system/$service.d"
    drop_in="$drop_in_directory/video-processing.conf"
    staged_file=$(mktemp)
    trap 'rm -f "$staged_file"' EXIT
    cat > "$staged_file" <<EOF
[Service]
Environment=TENRANKAI_FFMPEG=/usr/bin/ffmpeg
Environment=TENRANKAI_FFPROBE=/usr/bin/ffprobe
Environment=TENRANKAI_VIDEO_TEMP_DIR=$directory
EOF
    if [[ -e $drop_in ]] && ! cmp -s "$staged_file" "$drop_in"; then
        echo "Existing $drop_in differs; refusing to overwrite it." >&2
        exit 1
    fi
    install -d -m 0755 "$drop_in_directory"
    install -m 0644 "$staged_file" "$drop_in"
    rm -f "$staged_file"
    trap - EXIT
    echo "$service: staged scratch directory $directory"
done

# Private validation area for testing the standalone processing crate. This is
# separate from each service's runtime scratch directory and deployed checkout.
install -d -o atrus -g atrus -m 0700 "$scratch_root/validation"
systemctl daemon-reload
echo "Service environment staged. Running processes have not been restarted."
