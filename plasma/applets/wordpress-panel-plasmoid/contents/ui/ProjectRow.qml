/*
 * Fila de un proyecto activo: nombre, dominio y botón de detener.
 */
import QtQuick
import QtQuick.Layouts
import org.kde.plasma.components as PlasmaComponents
import org.kde.kirigami as Kirigami

RowLayout {
    id: row

    property string siteId
    property string siteName
    property string siteDomain

    signal stopRequested(string id)

    spacing: Kirigami.Units.smallSpacing

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 0
        PlasmaComponents.Label {
            text: row.siteName
            font.bold: true
            elide: Text.ElideRight
            Layout.fillWidth: true
        }
        PlasmaComponents.Label {
            text: row.siteDomain
            opacity: 0.6
            font.pointSize: Kirigami.Theme.smallFont.pointSize
            elide: Text.ElideRight
            Layout.fillWidth: true
        }
    }

    PlasmaComponents.Button {
        text: "Detener"
        icon.name: "media-playback-stop"
        onClicked: row.stopRequested(row.siteId)
    }
}
