application_id = 'com.lucamignatti.nova'

scalable_dir = 'hicolor' / 'scalable' / 'apps'
install_data(
  scalable_dir / ('@0@.svg').format(application_id),
  install_dir: get_option('datadir') / 'icons' / scalable_dir,
)

symbolic_dir = 'hicolor' / 'symbolic' / 'apps'
install_data(
  symbolic_dir / ('@0@-symbolic.svg').format(application_id),
  install_dir: get_option('datadir') / 'icons' / symbolic_dir,
)

missing_icons = [
  'symbolic/status/media-playlist-repeat-song-symbolic.svg',
  'symbolic/status/media-playlist-shuffle-symbolic.svg',
  'symbolic/devices/media-optical-symbolic.svg',
  'symbolic/emblems/emblem-favorite-symbolic.svg',
  'symbolic/actions/media-skip-backward-symbolic.svg',
  'symbolic/actions/media-skip-forward-symbolic.svg',
]

install_data(missing_icons, install_dir: get_option('datadir') / 'icons' / symbolic_dir)
