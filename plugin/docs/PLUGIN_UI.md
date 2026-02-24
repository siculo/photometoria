# Photometoria – Interfaccia Plugin Lightroom Classic

## Panoramica

Il plugin si articola in tre finestre principali (modeless dialog) più due finestre modali di sistema usate per le operazioni distruttive. La navigazione tra le finestre principali segue un flusso naturale: si parte dal Setup Server, si entra nella gestione dei Task, e da lì si può aprire la finestra di aggiunta foto. Le modali appaiono in risposta ad azioni specifiche dell'utente senza abbandonare il contesto corrente.

```
Setup Server → Task (master-detail) → [modale] Crea Job
                                     → [modale] Conferma Eliminazione Task
                                     → [modale] Conferma Annullamento Job
                    ↘
                  Aggiunta Foto
```

---

## Finestra 1 – Setup Server

Punto di ingresso del plugin. Permette di configurare l'indirizzo del server e visualizzarne lo stato.

### Sezione Connessione

Un campo di testo libero raccoglie host e porta del server nel formato `host:porta` (es. `192.168.1.50:8080`). Il valore viene salvato nelle preferenze di Lightroom tra una sessione e l'altra.

Affiancato al campo compare un pulsante **Verifica**. Il pulsante rimane disabilitato finché il campo è vuoto; si abilita non appena viene digitato qualcosa.

Al click su Verifica il plugin tenta una connessione al server. Durante l'attesa compare una pill di stato "Verifica…". L'esito produce due comportamenti distinti:

- **Connessione riuscita**: pill verde "Online", la sezione Dettagli Server appare sotto, i pulsanti Salva e "Vai ai Task" si abilitano.
- **Connessione fallita**: pill rossa "Non raggiungibile", i dettagli rimangono nascosti, Salva e "Vai ai Task" restano disabilitati.

Se l'utente modifica il campo host dopo una verifica andata a buon fine, lo stato si azzera e i dettagli si nascondono di nuovo, richiedendo una nuova verifica. Questo evita che vengano salvate configurazioni non più valide.

### Sezione Dettagli Server

Visibile solo dopo una verifica con esito positivo. Mostra le informazioni restituite dal server:

- **Spazio allocato**: barra orizzontale con etichette numeriche (GB allocati su GB totali).
- **Spazio utilizzato dalle foto**: seconda barra che mostra l'occupazione reale dei dati caricati.
- **Provider disponibili**: lista dei provider configurati sul server, mostrati come chip testuali.
- **Provider di default**: evidenziato con colore accentuato tra i chip dei provider.
- **Versione server**, **numero di task attivi**, **job in coda**: valori testuali.

### Pulsanti

- **Annulla**: chiude senza salvare.
- **Vai ai Task**: porta alla finestra Task (abilitato solo dopo verifica riuscita).
- **Salva**: salva l'host nelle preferenze (abilitato solo dopo verifica riuscita).

---

## Finestra 2 – Task

Finestra principale del plugin. Raccoglie in un unico pannello la lista dei task (colonna sinistra) e il dettaglio del task selezionato (colonna destra), seguendo il pattern master-detail.

### Colonna sinistra – Lista Task

Ogni elemento della lista mostra il nome del task, un sommario sintetico (numero di foto e peso in GB) e una pill colorata che indica lo stato corrente:

| Stato | Significato |
|-------|-------------|
| Arancione "Attivo" | Almeno un job in corso |
| Verde "Completato" | Tutti i job terminati con successo |
| Rosso "Errori" | Job terminati con foto fallite |

La selezione di un task aggiorna immediatamente il pannello di destra senza navigazione aggiuntiva.

In cima alla colonna si trova il pulsante **+ Aggiungi foto**, che apre la finestra Aggiunta Foto. In fondo alla colonna:

- **Mostra in Libreria**: seleziona nel catalogo Lightroom le foto appartenenti al task selezionato.
- **Elimina**: rimuove il task e tutti i suoi dati dal server. Disabilitato se il task ha job attivi; in quel caso un testo esplicativo informa l'utente del motivo. Il click apre la modale di conferma (vedi sotto).

### Colonna destra – Dettaglio Task

Divisa internamente in due parti affiancate.

**Parte sinistra – Contesto**

Un'area di testo modificabile a tutta altezza contiene la descrizione del task, cioè le informazioni di contesto che il modello utilizzerà durante l'analisi delle foto (luogo, evento, periodo, stile, ecc.).

I pulsanti **Salva** e **Annulla** compaiono in fondo al campo solo quando il testo viene modificato, scompaiono dopo il salvataggio o l'annullamento. Annulla ripristina il testo salvato in precedenza.

**Parte destra – Job**

Lista dei job del task selezionato. Per ogni job è visibile:

- Provider e modello usato (es. `Ollama · qwen2-vl:8b`).
- Avanzamento: barra di progresso con contatore foto e stima del tempo rimanente per i job in corso; oppure riepilogo finale (foto totali, tempo impiegato, eventuali errori) per i job terminati.
- Pill di stato: In corso (arancione), Completato (verde), Con errori (rosso), Annullato (grigio).

In fondo alla colonna job si trovano i pulsanti di azione, abilitati dinamicamente in base al job selezionato:

| Pulsante | Condizione di attivazione |
|----------|--------------------------|
| Riprova Fallite | Job terminato con almeno una foto fallita |
| Applica Tag alle Foto | Job terminato con successo |
| Annulla Job | Job in stato "In corso" |
| + Nuovo Job | Sempre disponibile |

---

## Finestra 3 – Aggiunta Foto

Aperta tramite il pulsante "+ Aggiungi foto" nella finestra Task. Guida l'utente nella scelta di quali foto aggiungere e a quale task destinarle.

### Scelta delle foto

Due opzioni radio:

- **Solo selezionate** (default, disponibile solo se esiste una selezione attiva nel catalogo): aggiunge solo le foto selezionate in Lightroom al momento dell'apertura.
- **Tutte**: aggiunge tutte le foto del catalogo o della collezione attiva.

Sotto le opzioni, un riquadro di riepilogo aggiornato in tempo reale mostra il numero di foto che verrebbero aggiunte e il peso stimato.

### Scelta della destinazione

Due opzioni radio alternative:

**Nuovo task**: mostra due campi:
- *Nome* (obbligatorio): identificatore del task nel plugin.
- *Contesto* (facoltativo): descrizione delle foto per orientare il modello.

Un'annotazione informativa ricorda che un contesto ben compilato migliora la qualità dei tag generati.

**Task esistente**: mostra un menu a tendina con i task presenti sul server e un riquadro di riepilogo che mostra il numero di foto già presenti nel task selezionato e la stima dopo l'aggiunta.

### Pulsanti

- **Annulla**: chiude senza fare nulla.
- **Conferma e Vai al Task**: crea il task (se nuovo) o aggiunge le foto al task esistente, poi porta direttamente alla finestra Task con il task interessato già selezionato. Rimane disabilitato finché si è in modalità "Nuovo task" e il campo Nome è vuoto.

---

## Modale – Crea Job

Aperta tramite il pulsante "+ Nuovo Job" nella finestra Task.

### Scelta del modello

Due menu a tendina in cascata: il primo seleziona il provider, il secondo mostra i modelli disponibili per quel provider (la lista si aggiorna dinamicamente al cambio di provider).

Per i provider cloud (es. OpenAI, Anthropic) compare una riga informativa che riporta la stima del costo per l'analisi dell'intero batch di foto, calcolata prima dell'avvio.

### Opzioni

Una checkbox permette di attivare l'**applicazione automatica dei tag** al termine del job: se selezionata, al completamento i tag vengono scritti nel campo Keywords di Lightroom senza richiedere ulteriori azioni da parte dell'utente.

### Riepilogo

Un pannello in fondo alla modale mostra, in sola lettura, un riassunto della configurazione prima di avviare: numero di foto, modello selezionato, stato dell'opzione di applicazione automatica.

### Pulsanti

- **Annulla**: chiude senza creare il job.
- **▶ Avvia Job**: crea il job e lo mette in coda sul server.

---

## Modale – Conferma Eliminazione Task

Aperta dal pulsante Elimina nella finestra Task.

Mostra il nome del task che si sta per eliminare e i suoi dati sintetici (numero di foto, peso, numero di job). Un avviso rosso precisa che verranno eliminati dal server tutti i dati del task (job e foto caricate) e che le foto originali nel catalogo Lightroom non verranno toccate. L'operazione è irreversibile.

**Pulsanti:**
- **Annulla**: chiude senza eliminare.
- **Elimina definitivamente**: procede con l'eliminazione.

La modale si chiude anche cliccando fuori di essa.

---

## Modale – Conferma Annullamento Job

Aperta dal pulsante Annulla Job nella finestra Task.

Mostra il provider e il modello del job selezionato e il suo stato corrente. Un avviso ambra informa che il job si interromperà al termine dell'elaborazione della foto in corso e che i risultati parziali già ottenuti resteranno disponibili.

**Pulsanti:**
- **Indietro**: chiude senza annullare (il termine "Indietro" è intenzionale per evitare ambiguità con "Annulla il job").
- **Annulla il job**: procede con l'interruzione.

La modale si chiude anche cliccando fuori di essa.

---

## Note di progettazione

**Pattern master-detail per la finestra Task.** La scelta di unire lista task e dettaglio in un'unica finestra deriva dall'osservazione che le due schermate separate erano troppo legate per vivere autonomamente: la navigazione avanti-indietro con il pulsante "Torna ai Task" era un segnale di accoppiamento eccessivo. Il pattern master-detail permette di vedere sempre il contesto completo senza cambiare schermata.

**Conferme prima delle operazioni distruttive.** Sia l'eliminazione del task che l'annullamento del job richiedono una conferma esplicita tramite modale dedicata. Il testo dei pulsanti di conferma è deliberatamente descrittivo ("Elimina definitivamente", "Annulla il job") per ridurre il rischio di click accidentali. La modale di eliminazione usa il rosso, quella di annullamento job usa l'ambra, perché le conseguenze sono asimmetriche: l'eliminazione è irreversibile, l'annullamento lascia disponibili i risultati parziali.

**Dettagli server condizionali.** Le informazioni sul server vengono mostrate solo dopo una verifica esplicita della connessione. Questo evita di mostrare dati potenzialmente non aggiornati e rende evidente all'utente quando la configurazione non è stata ancora validata.

**Pulsante Conferma disabilitato senza nome task.** Nella finestra di aggiunta foto, il pulsante di conferma è disabilitato finché si è in modalità "Nuovo task" e il campo Nome è vuoto. Il campo Contesto non è bloccante perché può essere compilato in seguito dalla finestra Task.

**Applicazione automatica dei tag come opt-in.** La checkbox nella creazione job è deselezionata di default: scrivere metadati nel catalogo Lightroom è un'azione significativa e l'utente potrebbe voler revisionare i risultati prima. Chi vuole il comportamento automatico può attivarlo esplicitamente.

---

*Documento aggiornato in concomitanza con il prototipo `PLUGIN_UI_PROTOTYPE.html`.*
