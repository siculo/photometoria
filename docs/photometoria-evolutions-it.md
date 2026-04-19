# Photometoria — Possibili Evoluzioni

Riepilogo della discussione sulle direzioni future del progetto, esplorata a febbraio 2026.

## Stato attuale

Il progetto ha completato la prima milestone: i servizi principali della REST API sono implementati, lo storage è stato migrato da in-memory a persistente basato su file JSON. Il repository è https://github.com/siculo/photometoria.

Il sistema attuale analizza singole foto tramite modelli di visione (qwen2-vl:8b via Ollama) e genera tag descrittivi, considerando anche il contesto fornito dall'utente nel Task.

---

## Direzione 1: Analisi oltre la singola foto

L'idea di fondo è superare l'analisi indipendente di ogni foto per costruire relazioni tra foto. Questo si articola su tre assi:

**Soggetto (chi):** richiede face recognition cross-foto. Non basta riconoscere che in una foto c'è un volto — serve collegare lo stesso volto attraverso foto diverse e associargli un'identità (con intervento dell'utente).

**Luogo (dove):** già parzialmente raggiungibile con due fonti: il tagging visuale (il modello riconosce un ristorante, un monumento) e le coordinate GPS dai dati EXIF. Le due informazioni sono complementari — il GPS dà il "dove preciso", il modello dà il "dove semantico".

**Evento (cosa):** è il concetto più sfuggente perché emerge dall'incrocio degli altri dati. "Il compleanno di Marco" non si vede in una singola foto ma si ricostruisce dalla combinazione di prossimità temporale, luogo, soggetti presenti, e contesto fornito dall'utente.

Questi tre assi definiscono livelli di astrazione crescente: **fatti** dalla singola foto (tag, volti, GPS, data), **relazioni** tra foto (stesso soggetto, stesso luogo, vicinanza temporale), **concetti** che emergono dalla combinazione (eventi, viaggi, occasioni).

---

## Direzione 2: Face Recognition

### Pipeline standard

Il riconoscimento facciale cross-foto segue un flusso consolidato:

1. **Face detection** — individuare i volti in ogni foto (bounding box e landmark facciali)
2. **Face embedding** — per ogni volto, generare un vettore numerico (128-512 dimensioni) che lo rappresenta in uno spazio geometrico
3. **Clustering** — raggruppare embedding simili per identificare "questa è la stessa persona in N foto" (algoritmi come DBSCAN/HDBSCAN)
4. **Labeling** — l'utente assegna un nome a ciascun cluster

### Modelli di face detection

- **RetinaFace:** stato dell'arte per accuratezza, architettura Feature Pyramid Network, restituisce bounding box + 5 landmark facciali. Ideale per batch processing offline.
- **MTCNN:** cascata di tre reti (P-Net, R-Net, O-Net), più leggero di RetinaFace ma meno accurato sui casi difficili.
- **MediaPipe (BlazeFace):** framework Google ottimizzato per real-time/mobile, meno adatto per analisi batch di qualità.

### Nota importante

Il face recognition **non** usa i modelli di visione generativa già in uso (qwen, llava). Richiede modelli specializzati, più leggeri e veloci, che producono embedding numerici. È un pipeline completamente separato dal tagging.

### Integrazione in Photometoria

L'ipotesi è trattare il face recognition come un nuovo tipo di provider (es. `FaceProvider`), separato dal `VisionProvider` esistente. Librerie come **InsightFace** (Python, integra RetinaFace + ArcFace) potrebbero essere incapsulate in un microservizio HTTP, seguendo lo stesso pattern di comunicazione usato con Ollama.

Il concetto di Job si estende naturalmente: oltre ai job di tagging si avrebbero job di face extraction, con output diversi (embedding e bounding box anziché tag testuali). Il clustering e il labeling restano responsabilità di Photometoria, non del provider.

---

## Direzione 3: Ricerca semantica

Per supportare query in linguaggio naturale come "le foto del compleanno di Marco al ristorante" servono due componenti:

**Metadati strutturati + ricerca tradizionale:** tag in database con indici, cluster facciali con associazioni foto↔persona, query combinabili.

**Embedding multimodali + ricerca vettoriale:** modelli come CLIP generano un embedding dell'intera foto che ne cattura il "significato" complessivo. Permette ricerche in linguaggio naturale anche senza tag esatti. Richiede un vector database (ChromaDB, Qdrant, pgvector).

**Approccio ibrido (raccomandato):** embedding facciali per le persone, tag strutturati per luoghi e caratteristiche, embedding CLIP per la ricerca semantica fuzzy. La query "foto a Parigi con Tizio" diventa una combinazione di filtri su cluster persona, tag di luogo, e prossimità nello spazio vettoriale.

---

## Panorama degli strumenti analoghi

### Self-hosted completi
- **Immich** — il più completo: face recognition, ricerca CLIP, backup mobile, multi-user. Architettura a microservizi (Node.js + Python ML + PostgreSQL). Stabile dalla v2.0 (ottobre 2025).
- **PhotoPrism** — AI tagging e ricerca, scritto in Go, più focalizzato su esplorazione web della libreria.
- **HomeGallery** — più leggero, Node.js, ricerca per immagini simili e face discovery.

### Desktop open source
- **digiKam** — veterano KDE, face recognition, batch processing, supporto EXIF/IPTC/XMP completo, gestisce librerie >100k immagini.
- **Shotwell** — semplice, niente AI, organizzazione per data.

### Strumenti di tagging
- **STAG** — Apache 2.0, nato da esigenza simile a Photometoria: tagging batch locale con modello ML (recognize-anything), output in XMP sidecar. Tool standalone, non servizio API.
- **OpenPhotos** — alternativa locale a Google Photos con face recognition (DeepFace) e Ollama. Concettualmente vicino a Photometoria.

### Commerciali/Enterprise
- **Adobe Experience Manager Smart Tagging** — AI trainabile su vocabolario aziendale, enterprise.
- **Daminion** — DAM commerciale con face recognition e propagazione identità.
- **CYME, ioMoVo, PhotoTag.ai, Auto Metadata AI** — servizi cloud per stock photography e DAM aziendale.

### Posizionamento di Photometoria
Photometoria non è una galleria (Immich/PhotoPrism), non è un DAM enterprise, non è un tool standalone (STAG). È un **servizio API di analisi foto** pensato per integrarsi con workflow esistenti (Lightroom e altri), con supporto per provider AI multipli e processing locale. Questa natura di servizio integrabile è un possibile punto di forza.

---

## Direzione 4: Estrazione testo dalle foto

Il caso d'uso è emerso osservando i tag generati per una foto di un cartellone pubblicitario: il modello ha descritto il soggetto ("cartellone", "pubblicità") ma non ha catturato il testo effettivamente visibile nell'immagine. Estrarre quel testo aggiungerebbe uno strato di metadati qualitativamente diverso.

### Cosa significa

L'estrazione testo differisce dal tagging semantico: i tag descrivono il *significato* di ciò che è nella foto, l'estrazione testo cattura il *contenuto testuale letterale* visibile nell'immagine — insegne, cartelloni, menu, etichette, segnali stradali, documenti fotografati, testo sugli edifici.

### Perché si integra nell'architettura esistente

I modelli di visione già in uso (qwen2-vl e simili) sono capaci di leggere testo dalle immagini senza richiedere provider aggiuntivi né modelli OCR specializzati. L'estrazione testo si implementerebbe come nuovo tipo di activity (`text_extraction`), riutilizzando l'infrastruttura di provider e worker esistente.

### Output e integrazione

Il testo estratto potrebbe essere salvato come:
- **Keywords/tag** in Lightroom (stesso flusso del tag extraction)
- **Campo didascalia o descrizione** per contenuto testuale più lungo
- **Campi strutturati** se il testo ha un formato riconoscibile (es. un'insegna con un nome di luogo)

### Considerazioni

- La qualità varia con la risoluzione dell'immagine e la leggibilità del font — il contesto dell'activity (vedi issue #122) potrebbe guidare il modello ("cerca il testo del cartellone", "estrai le voci del menu")
- Il testo in lingue miste (es. insegne straniere fotografate all'estero) è gestito naturalmente dai modelli multilingua
- La funzione è complementare al tag extraction: i due tipi di activity si possono eseguire indipendentemente o in sequenza sullo stesso progetto

---

## Considerazioni sul deployment

Il target di Photometoria comprende fotografi (professionisti e appassionati) che generalmente non sono tecnici. La complessità di setup è l'ostacolo principale all'adozione.

**Docker Compose** è la strada più realistica: un singolo file che orchestra tutti i componenti (server Rust, Ollama con modelli, eventuali servizi Python, database). È il pattern adottato da Immich, PhotoPrism, HomeGallery.

Una **configurazione default opinionata** è importante: stack preconfigurato che funziona out of the box (es. Ollama + modello specifico), con la possibilità di configurazione avanzata per chi sa cosa fa.

Ogni provider aggiuntivo (servizio Python per face recognition, vector database per ricerca semantica) aggiunge complessità di deployment che va gestita.

---

## Natura del progetto

Photometoria è un progetto open source (Apache 2.0) nato per soddisfare un'esigenza personale, per curiosità, per fare esperienza e imparare. Non è un'iniziativa commerciale. Le direzioni future cresceranno al ritmo della curiosità e dell'interesse dell'autore, il che è coerente con la natura del progetto.

---

*Documento generato il 13 febbraio 2026*
