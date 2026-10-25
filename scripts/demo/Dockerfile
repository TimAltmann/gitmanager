# Aufnahme-Umgebung für Screenshots und Videos (scripts/demo/record.sh):
# Entwicklungs-Image + virtuelles Display, Bedienung per xdotool, ffmpeg.
FROM gitmanager-dev:latest
RUN apt-get update && apt-get install -y --no-install-recommends \
      xvfb xdotool ffmpeg x11-utils imagemagick \
      libgl1 libgl1-mesa-dri libegl1 libxkbcommon-x11-0 libxcursor1 libxrandr2 libxi6 \
    && rm -rf /var/lib/apt/lists/*
