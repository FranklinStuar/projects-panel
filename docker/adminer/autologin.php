<?php
/**
 * Plugin de Adminer para el panel WP — auto-login (entorno de desarrollo).
 *
 * Adminer solo escucha en 127.0.0.1 (ver `ensure_adminer` en docker.rs) y la
 * contraseña de todos los containers DB del panel es `panel` (ver `db_env`).
 * El panel abre Adminer con `?server=…&username=…&db=…` (mysql/maria) o
 * `?pgsql=…` (postgres), por lo que aquí inyectamos las credenciales y se entra
 * sin pasar por el formulario de login.
 *
 * Cómo funciona (Adminer 4): los plugins se construyen (adminer.php:~1193) antes
 * del bloque de autenticación (~1428). Si en una petición GET no hay un POST de
 * `auth`, fabricamos uno con la contraseña del entorno. Adminer, al ver un POST
 * de `auth` con token no vacío, lo reemplaza por el token de sesión válido
 * (`$_POST["token"]=$oi`), así `verify_token()` pasa. Resultado: login en cero
 * clics.
 *
 * Solo en GET: en POST (ejecutar SQL, alterar tablas…) NO inyectamos, para no
 * pisar el formulario real del usuario — esas peticiones ya van autenticadas por
 * la cookie de sesión que dejó el primer GET.
 *
 * Sin restricción de vista: si el servidor o el nombre de la DB es incorrecto,
 * la conexión falla de forma natural (justo lo que se espera en dev).
 */
class AdminerPanelAutologin {
	function __construct() {
		if (
			($_SERVER["REQUEST_METHOD"] ?? "GET") === "GET"
			&& !isset($_POST["auth"])
			&& isset($_GET["username"])
		) {
			$is_pg = isset($_GET["pgsql"]);
			$driver = $is_pg ? "pgsql" : "server";
			$server = $is_pg
				? $_GET["pgsql"]
				: (isset($_GET["server"]) ? $_GET["server"] : "");
			$_POST["auth"] = array(
				"driver" => $driver,
				"server" => $server,
				"username" => $_GET["username"],
				"password" => "panel",
				"db" => isset($_GET["db"]) ? $_GET["db"] : "",
			);
			// Token cualquiera no vacío: Adminer lo sustituye por el válido.
			$_POST["token"] = "1";
		}
	}
}

return new AdminerPanelAutologin;
