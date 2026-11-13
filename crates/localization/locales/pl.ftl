connection-connecting = Łączenie z { $address }…
connection-disconnected = Rozłączono
connection-starting-local = Uruchamianie lub podłączanie do serwera lokalnego…
connection-awaiting-welcome = Oczekiwanie na powitanie serwera
connection-not-connected = Brak połączenia
connection-no-response = Brak odpowiedzi serwera
terminal-intro = Konsola Minimium. Wpisz `help`, aby wyświetlić polecenia.
terminal-prompt = >
window-title = Klient 3D Minimium
window-create-failed = Nie udało się utworzyć okna: { $detail }
connection-attempt-busy = Próba połączenia { $id } nadal się kończy; spróbuj ponownie za chwilę
connection-attempts-exhausted = Nie można rozpocząć połączenia: wyczerpano identyfikatory prób
connection-worker-start-failed = Nie można uruchomić zadania połączenia: { $detail }
connection-timeout = Przekroczono czas połączenia; oczekiwanie na zakończenie przed ponowną próbą
connection-waiting-cleanup = Oczekiwanie na zakończenie anulowanego połączenia
connection-setup-failed = Przygotowanie połączenia nie powiodło się: { $detail}; możesz połączyć się ponownie
connection-failed = Połączenie nie powiodło się: { $detail}; możesz spróbować ponownie
connection-disconnect-or-wait = Najpierw rozłącz się lub poczekaj na zakończenie bieżącej próby połączenia
command-queued = Polecenie dodane do kolejki
client-error = Błąd klienta: { $detail }
map-load-failed = Nie można wczytać mapy { $path }: { $detail }
bundled-map-invalid = Nieprawidłowa mapa wbudowana: { $detail }
graphics-init-failed = Inicjalizacja grafiki nie powiodła się: { $detail }
renderer-already-initialized = Renderer można zainicjalizować tylko raz
render-failed = Błąd renderowania: { $detail }
fps-unavailable = Podgląd FPS nie jest dostępny w kliencie terminalowym
console-history-help = F1 zamyka konsolę. Strzałki przewijają historię poleceń.
shot-fired = Oddano strzał
visible-player-limit-exceeded = Serwer przekroczył limit widocznych graczy w migawce
welcome-timeout = Przekroczono czas oczekiwania na powitanie serwera; możesz połączyć się ponownie
heartbeat-timeout = Przekroczono czas oczekiwania na sygnał serwera; możesz połączyć się ponownie
peer-disconnected = Serwer rozłączył połączenie; możesz połączyć się ponownie
server-closed = Serwer zamknął sesję; możesz połączyć się ponownie
connection-rejected = Serwer odrzucił połączenie: { $reason ->
    [protocol] niezgodny protokół
    [busy] serwer jest zajęty; przyczyną może być aktywny profil
    [full] serwer jest pełny
    [rate_limit] przekroczono limit żądań
   *[other] serwer odrzucił żądanie
}
server-error = Błąd serwera: { $reason ->
    [protocol] niezgodny protokół
    [busy] serwer jest zajęty
    [full] serwer jest pełny
    [rate_limit] przekroczono limit żądań
   *[other] odrzucono żądanie
}
map-mismatch = Niezgodność mapy: klient={ $expected } serwer={ $actual }
connection-lost = Utracono połączenie ({ $detail }); możesz połączyć się ponownie
connection-connected = Połączono jako gracz { $id }, zdrowie { $health } HP
network-ping = Opóźnienie: { $milliseconds } ms
network-pong = PONG
server-accepted = Serwer zaakceptował polecenie
player-health = Zdrowie: { $health } HP
shot-missed = Pudło
shot-hit = Trafiono gracza { $target } ({ $health } HP)
shot-target-respawned = Trafiono gracza { $target }; cel odrodził się ({ $health } HP)
queue-failure = { $action ->
    [command] Nie udało się dodać polecenia do kolejki
    [fire] Nie udało się oddać strzału
   *[movement] Nie udało się dodać ruchu do kolejki
}: { $cause ->
    [disconnected] brak połączenia
    [awaiting] oczekiwanie na powitanie
    [authentication] trwa uwierzytelnianie
    [stale] nieaktualny kontekst świata
    [full] kolejka wysyłkowa jest pełna
    [sequence] wyczerpano numery wejść ruchu; połącz się ponownie
   *[protocol] protokół odrzucił wiadomość
}
window-title-status = Minimium | { $fps } FPS | { $health } HP | WASD ruch, Spacja skok, lewy przycisk strzał, Esc zwolnij kursor
terminal-snapshot = czas_serwera={ $time }ms pełna={ $full } pozycja=({ $position }) prędkość=({ $velocity }) na_ziemi={ $grounded } aktualizacje_graczy={ $updates } usunięcia_graczy={ $removals }
player-state = pozycja=({ $position }) zdrowie={ $health } HP pobliscy_gracze={ $nearby }
network-stats = wysłano={ $sent } bajtów odebrano={ $received } bajtów RTT={ $ping }{ $nearby_active ->
    [yes]  pobliscy_gracze={ $nearby }
   *[no] { "" }
}
chat-line = [{ $name }#{ $id }] { $text }
altitude-result = { $result ->
    [success] set_altitude: osiągnięto cel
   *[rejected] Odrzucono set_altitude: wymagany jest włączony lot, planeta i prawidłowa wysokość
}
world-transfer-prepared = Przygotowano zmianę świata
world-transfer-committed = Zatwierdzono zmianę świata
world-transfer-cancelled = Anulowano zmianę świata
items-available = { $count ->
    [one] Dostępny { $count } element
    [few] Dostępne { $count } elementy
    [many] Dostępnych { $count } elementów
   *[other] Dostępne: { $count }
}
ui-status = { $status ->
    [earth_connecting] Łączenie z Earth...
    [earth_disconnected] Earth: rozłączono
    [earth_connected] Earth: połączono
    [connected] Połączono
    [earth_loading] Earth: wczytywanie terenu...
    [earth_choose_land] Earth: wybierz punkt na lądzie
    [earth_click_land] Earth: kliknij ląd na globie (Esc anuluje)
    [earth_click_surface] Earth: kliknij powierzchnię globu
    [earth_moving] Earth: przenoszenie...
    [earth_moved] Earth: przeniesiono
    [terrain_materials] Przygotowywanie materiałów terenu
    [connection_timed_out] Przekroczono czas połączenia
    [earth_server_closed] Earth: serwer zamknął połączenie
    [content_unavailable] Zawartość świata jest niedostępna
    [environment_initializing] Inicjowanie
    [environment_synchronized] Zsynchronizowano
    [environment_stale] Nieaktualne
    [environment_unavailable] Niedostępne
    [environment_local_demo] Lokalna demonstracja
   *[other] { $status }
}
earth-error = Błąd Earth: { $detail }
ui-notice = { $notice ->
    [console_welcome] F1 otwiera konsolę poleceń. Wpisz help, aby zobaczyć polecenia.
    [console_overview] M przełącza widok przeglądu świata.
    [commands_header] Polecenia (PageUp/PageDown przewijają listę):
    [time_request_sent] Wysłano żądanie zmiany czasu
    [time_scale_request_sent] Wysłano żądanie zmiany skali czasu
    [bolid_request_sent] Wysłano żądanie bolidu
    [bolid_request_queued] Żądanie bolidu czeka na gotowość świata
    [time_mode_request_sent] Wysłano żądanie zmiany trybu czasu
    [time_offline_policy_request_sent] Wysłano żądanie zmiany polityki czasu offline
    [sky_bodies_unavailable] Brak danych o ciałach niebieskich
    [map_teleport_desktop_only] map_telepo jest dostępne w graficznym przeglądzie Earth
    [sky_info_desktop_only] Informacje o niebie wymagają klienta graficznego
    [weather_requires_desktop] Diagnostyka pogody regionalnej wymaga klienta graficznego
    [meteors_require_desktop] Polecenia meteorów otoczenia są dostępne tylko w kliencie graficznym
    [sky_quality_desktop_only] --sky-quality jest dostępne tylko w kliencie graficznym
    [map_teleport_busy] map_telepo: poczekaj na poprzednie przeniesienie
    [map_teleport_requires_earth] map_telepo wymaga połączenia ze światem Earth
    [local_server_threads_conflict] server.threads podano zarówno w opcjach klienta, jak i poleceniu hosta
    [star_catalog_unavailable] Katalog gwiazd jest niedostępny; etykiety ciał niebieskich są wyłączone
    [time_request_failed] Nie udało się wysłać żądania zmiany czasu
    [time_scale_request_failed] Nie udało się wysłać żądania zmiany skali czasu
    [time_mode_request_failed] Nie udało się wysłać żądania zmiany trybu czasu
    [time_offline_policy_request_failed] Nie udało się wysłać żądania zmiany zasady czasu offline
    [time_scale_response_failed] Serwer nie zastosował żądanej skali czasu
    [environment_unavailable] Dane środowiska są tymczasowo niedostępne
    [clipboard_unavailable] Schowek jest niedostępny
    [altitude_requires_flight] set_altitude wymaga włączonego latania
    [portal_snapshot_unavailable] Interakcja z portalem wymaga migawki gracza
    [portal_unavailable] Brak pobliskiego portalu
    [local_server_config_failed] Nie udało się przygotować konfiguracji lokalnego serwera
   *[other] { $notice }
}
property-description = { $key ->
    [ui_locale] Język interfejsu użytkownika.
    [client_camera_fov] Pionowe pole widzenia kamery.
    [client_input_mouse_sensitivity] Czułość myszy; zero wyłącza obrót kamerą.
    [client_input_bindings] Fizyczne skróty klawiszowe w kontekstach wejścia; konflikty w tym samym kontekście są odrzucane.
    [client_graphics_sky_quality] Lokalna jakość cieni i renderowania nieba.
    [client_sky_names] Pokazuje etykiety ciał niebieskich w kliencie graficznym.
    [client_audio_meteors] Włącz opcjonalny opóźniony huk meteorów w tej sesji.
    [client_audio_meteor_volume] Głośność meteorów (od 0 do 1).
    [client_weather_cloud_detail] Maksymalna głębokość rekurencji wokseli chmur.
    [client_debug_atmosphere] Diagnostyczny widok atmosfery.
    [client_debug_terrain] Diagnostyczny widok terenu.
    [client_debug_clouds] Diagnostyczna nakładka chmur.
    [client_sky_demo] Włącza lokalną prezentację ciał niebieskich.
    [player_movement_fly] Lokalny stan lotu/noclip; przyjęcie do kolejki nie potwierdza go przez serwer.
    [player_movement_speed] Lokalna prędkość ruchu; przyjęcie do kolejki nie potwierdza jej przez serwer.
    [player_movement_accel] Lokalne przyspieszenie ruchu; przyjęcie do kolejki nie potwierdza go przez serwer.
    [world_time_mode] Tryb zegara świata; zmiana wymaga kanału administracyjnego lokalnego serwera.
    [world_time_scale] Tempo zegara świata; zmiana wymaga kanału administracyjnego lokalnego serwera.
    [world_time_offline_policy] Zasada działania zegara świata poza grą.
    [world_time_utc] Data UTC świata zsynchronizowana z repliką klienta.
    [world_time_revision] Rewizja zegara odebrana z repliki klienta.
   *[other] { $key }
}
no-property-matches = Brak właściwości klienta pasujących do { $prefix }
command-help = { $name ->
    [terrain_status] terrain status
    [atmosphere_status] atmosphere status
    [atmosphere_debug] atmosphere debug off|transmission|scattering|optical_depth|distance|fog
    [terrain_debug] terrain debug off|albedo|normals|depth|shadow|base_color|coverage|habitat|source
    [weather_status] weather status
    [weather_clouds_debug] weather clouds debug off|regions|layers|wind|kinds|detail
    [weather_clouds_detail] weather clouds detail 2|3|4|5|6
    [set_time] set_time HHMM (UTC; lokalny serwer hosta)
    [set_time_local] set_time_local HHMM (lokalny średni czas słoneczny gracza; lokalny serwer hosta)
    [sky_quality] sky quality low|standard
    [sky_info] sky info
    [sky_bodies] sky bodies
    [sky_demo] sky demo on|off (lokalny pokaz renderowania)
    [camera_fov] camera fov [30..110] (pionowe pole widzenia w stopniach; domyślnie 70)
    [sky_names] sky names true|false
    [time_status] time status
    [time_scale] time scale DECIMAL (alias timescale; lokalny serwer hosta; 0.1-1000000x lub 0)
    [spawn_random_bolid] spawn_random_bolid (lokalny serwer hosta)
    [help] help
    [config_save] config save PATH
    [diagnostics_fps] diagnostics fps
    [network_stats] network stats
    [diagnostics_state] diagnostics state
    [console_clear] console clear
    [console_quit] console quit (alias: exit)
    [movement_move] movement move X Z (-1..1 każda; kierunek styczny)
    [movement_jump] movement jump
    [world_portal] world portal
    [world_teleport] world teleport: kliknij ląd w przeglądzie Earth
    [combat_fire] combat fire
    [social_say] social say TEXT (alias: chat)
    [social_name] social name NAME
    [network_host_local] network host local [--threads 1|2|auto]
    [network_connect] network connect HOST:PORT
    [network_disconnect] network disconnect
    [network_ping] network ping
    [set_altitude] set_altitude <0..10000 km> — wysokość nad lokalnym terenem; wymaga włączonego lotu
    [movement_fly] movement fly [on|off] — noclip; Spacja w górę, Ctrl w dół; używa prędkości i przyspieszenia chodu
    [movement_speed] movement speed [0..10000] — maksymalna prędkość chodu/lotu w m/s; bez argumentu pokazuje wartość
    [movement_accel] movement accel [0..10000] — przyspieszenie i hamowanie w m/s²; bez argumentu pokazuje wartość
   *[other] { $name }
}
command-rejected = { $code ->
    [unknown_command] Nieznane polecenie { $subject }.
    [invalid_arguments] Nieprawidłowe argumenty dla { $subject }.
    [unknown_key] Nieznana właściwość lub opcja { $subject }.
    [invalid_value] Nieprawidłowa wartość dla { $subject }.
    [missing_value] Podaj wartość dla { $subject }.
    [duplicate_option] Opcję { $subject } podano więcej niż raz.
    [conflicting_options] Sprzeczne opcje dotyczące { $subject }.
    [unknown_option] Nieznana opcja { $subject }.
    [help_query] Brak pomocy dla { $subject }.
    [permission_denied] Brak uprawnień do { $subject }.
    [unsupported_context] { $subject } nie jest dostępne w tym kliencie.
    [restart_required] { $subject } wymaga ponownego uruchomienia aplikacji.
    [creation_only] { $subject } można ustawić tylko podczas tworzenia świata.
    [read_only] { $subject } jest tylko do odczytu.
    [io] Nie udało się wykonać operacji dla { $subject }.
   *[other] Nie udało się wykonać polecenia.
}
map-teleport-result = { $result ->
    [success] map_telepo: osiągnięto cel
   *[rejected] map_telepo: cel jest niedostępny; wybierz ląd
}
content-unavailable = Nie udało się przygotować zawartości świata. Szczegóły są w logu klienta.
configuration-saved = Zapisano konfigurację w { $path }
configuration-save-failed = Nie udało się zapisać konfiguracji. Szczegóły są w logu klienta.
environment-status = Środowisko: { $quality } | { $date } | tempo { $rate } | rewizja { $revision }
environment-title = { $quality } | { $date }
