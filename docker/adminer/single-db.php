<?php
/**
 * Plugin de Adminer para el panel WP.
 *
 * Hace dos cosas, ambas pensadas para el entorno LOCAL del panel (Adminer solo
 * escucha en 127.0.0.1, ver `ensure_adminer` en docker.rs):
 *
 *  1. Auto-login: la contraseña de todos los containers DB del panel es `panel`
 *     (ver `db_env` en docker.rs). El usuario llega por la URL con
 *     `?server=…&username=…&db=…` (mysql/maria) o `?pgsql=…` (postgres); este
 *     plugin acepta el submit con contraseña vacía y aporta `panel`. Así basta
 *     pulsar «Login» una vez.
 *
 *  2. Restringe la vista a UNA base de datos: Adminer arrastra `server`,
 *     `username` y `db` como parámetros GET en cada enlace interno, así que
 *     limitar `databases()` al `db` de la URL mantiene oculto el resto de DBs
 *     del container compartido durante toda la sesión. No es seguridad real
 *     (root sigue teniendo grants), es acotar la vista al proyecto.
 *
 * El panel pasa el `db` correcto del proyecto (`services.db.dbName`), por lo que
 * cada proyecto abre directamente su propia base de datos.
 */
class AdminerPanelSingleDb {
	/** Credenciales de conexión: usuario del form, contraseña fija del entorno. */
	function credentials() {
		return array(SERVER, $_GET["username"], "panel");
	}

	/** Acepta el login (entorno local; password vacío válido). */
	function login($login, $password) {
		return true;
	}

	/** Solo la base de datos del proyecto; el resto del container queda oculto. */
	function databases($flush = true) {
		$only = isset($_GET["db"]) ? $_GET["db"] : "";
		return $only !== "" ? array($only) : get_databases($flush);
	}
}

return new AdminerPanelSingleDb;
