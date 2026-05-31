/*
 * Plasmoid Panel WP — lista los proyectos WordPress activos y permite detenerlos
 * o apagar todo y cerrar el panel.
 *
 * Habla con el daemon Tauri por D-Bus (com.goldmediatech.WordpressPanel) usando
 * qdbus6 a través del DataSource "executable" de Plasma. Sin "encender todos"
 * (requisito del usuario).
 */
import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.components as PlasmaComponents
import org.kde.plasma.plasma5support as P5Support
import org.kde.kirigami as Kirigami

PlasmoidItem {
    id: root

    readonly property string service: "com.goldmediatech.WordpressPanel"
    readonly property string path: "/com/goldmediatech/WordpressPanel"
    readonly property string iface: "com.goldmediatech.WordpressPanel.Manager"

    property var sites: []

    Plasmoid.icon: "applications-development"
    switchWidth: Kirigami.Units.gridUnit * 12
    switchHeight: Kirigami.Units.gridUnit * 10

    // --- Ejecutor de comandos (qdbus6) -------------------------------------
    P5Support.DataSource {
        id: exec
        engine: "executable"
        connectedSources: []
        property var pending: ({})

        onNewData: (sourceName, data) => {
            const stdout = (data["stdout"] || "").trim();
            const cb = pending[sourceName];
            disconnectSource(sourceName);
            delete pending[sourceName];
            if (cb) cb(stdout);
        }

        function run(cmd, cb) {
            pending[cmd] = cb || null;
            connectSource(cmd);
        }
    }

    function callDbus(method, args, cb) {
        let cmd = "qdbus6 " + service + " " + path + " " + iface + "." + method;
        for (const a of (args || [])) cmd += " '" + a + "'";
        exec.run(cmd, cb);
    }

    function refresh() {
        callDbus("GetRunningSites", [], (out) => {
            try {
                root.sites = JSON.parse(out || "[]");
            } catch (e) {
                root.sites = [];
            }
        });
    }

    Timer {
        interval: 3000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: root.refresh()
    }

    // --- Representación compacta (barra) -----------------------------------
    compactRepresentation: MouseArea {
        onClicked: root.expanded = !root.expanded
        Kirigami.Icon {
            anchors.fill: parent
            source: "applications-development"
        }
        PlasmaComponents.Label {
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            text: root.sites.length > 0 ? root.sites.length : ""
            font.bold: true
        }
    }

    // --- Representación completa (popup) -----------------------------------
    fullRepresentation: ColumnLayout {
        Layout.minimumWidth: Kirigami.Units.gridUnit * 16
        Layout.minimumHeight: Kirigami.Units.gridUnit * 14
        spacing: Kirigami.Units.smallSpacing

        PlasmaComponents.Label {
            Layout.fillWidth: true
            text: "Proyectos activos: " + root.sites.length
            font.bold: true
        }

        ListView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.sites
            spacing: Kirigami.Units.smallSpacing
            delegate: ProjectRow {
                width: ListView.view.width
                siteId: modelData.id
                siteName: modelData.name
                siteDomain: modelData.domain
                onStopRequested: (id) => {
                    root.callDbus("StopSite", [id], () => root.refresh());
                }
            }

            PlasmaComponents.Label {
                anchors.centerIn: parent
                visible: root.sites.length === 0
                text: "Ningún proyecto encendido"
                opacity: 0.6
            }
        }

        PlasmaComponents.Button {
            Layout.fillWidth: true
            text: "Apagar todo y cerrar"
            icon.name: "system-shutdown"
            enabled: root.sites.length > 0
            onClicked: {
                root.callDbus("StopAll", [], () => {
                    root.callDbus("Quit", [], () => {});
                });
            }
        }
    }
}
