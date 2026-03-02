Ciao, lavoriamo sul plugin lightroom contenuto in plugin/photometoria.lrdevplugin. In particolare sull'interfaccia di setup del server. L'interfaccia funziona, ma la configurazione del server non viene memorizzata nelle preferenze. Infatti, quando chiudo al finestra di gestione plug-in e la riapro, la configurazione del server risulta vuota.











Vorrei iniziare a realizzare l'interfaccia del plugin a partire da quanto realizzato nel prototipo presente in plugin/prototype.



In questo momento mi voglio concentrare sulla realizzazione dell'interfaccia di Setup del server.



Questa interfaccia va realizzata nel plug-in manager (File -> Gestione plug-in). Il pannello di setup del server va aggiunto in sectionsForTopOfDialog. Il pannello deve essere realizzato ricalcando il pannello "Photometoria – Setup Server" del prototipo. Quindi ci deve essere il campo per l'immissione dei dati di connessione al server, il pulsante "Connect", deve essere mostrato il risultato del tentativo di connessione e in caso di connessione effettuata i dettagli del server. Per il momento non è necessario che la connessione sia effettiva, ma basta una connessione simulata, come avviene nel prototipo. ovviamente, i pulsanti Cancel e Save non sono presenti.

