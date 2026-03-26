<!-- translation-source: architecture.md @ 96e7fe4f656ee757136800f432ac5c78e0c685d5 -->

> **Nota:** Questa è una traduzione della [versione inglese](architecture.md).
> In caso di discrepanze, fa fede l'originale.

# Architettura dell'API Photometoria

## Panoramica

L'API di Photometoria è costruita utilizzando un'architettura moderna async-first con Rust e il framework Axum. Il sistema è progettato per scalabilità, manutenibilità ed evoluzione futura attraverso livelli di astrazione e una struttura modulare.

**Principi di design chiave:**

- Design async-first con runtime Tokio
- Pattern worker pool per la gestione delle risorse GPU
- Storage basato su filesystem con livelli di astrazione per futura integrazione con database
- Struttura modulare: routes, handlers, services, models, storage
- Astrazioni basate su trait per garantire estensibilità futura

## Design del sistema principale

### Approccio di tagging multi-livello

Il concept di design originale prevede molteplici livelli di analisi:

1. **Analisi individuale (Micro)**: Ogni foto analizzata separatamente per dettagli specifici
    - Esempio: "golden gate bridge, tramonto, lunga esposizione, cavi di sospensione rossi"

2. **Analisi di gruppo (Macro)**: Foto analizzate insieme per un contesto più ampio
    - Esempio: 20 foto → "vacanza a san francisco, estate 2024, costa californiana"

3. **Suggerimenti contestuali dell'utente**: Informazioni fornite manualmente
    - Esempio: "viaggio nella California settentrionale"

4. **Metadati EXIF**: Informazioni tecniche estratte dai dati della fotocamera

**Implementazione attuale:**

La versione iniziale semplifica questo sistema in un tagging a livello singolo che produce un unico set di tag per foto, tenendo comunque in considerazione:

- Il contenuto individuale della foto
- I suggerimenti contestuali forniti dall'utente

### Selezione e test dei modelli

**Risultati chiave:**

- **qwen3.5**: Modello raccomandato per la produzione — alta qualità, buon bilanciamento tra velocità e precisione
- **qwen3-vl**: Qualità superiore per l'identificazione di punti di riferimento e tagging dettagliato, ma più lento
- **gemma3n:e4b**: Modello vision Google Gemma 3n, testato come alternativa
- **ministral-3:latest**: Modello vision Mistral, testato come alternativa
- **llava**: Iterazione più veloce per lo sviluppo, qualità accettabile

**Approccio tecnico:**

- Utilizzo diretto dell'API HTTP di Ollama (più affidabile delle chiamate a sottoprocessi)
- I modelli restituiscono JSON strutturato (`{"tags": [{"tag": "..."}]}`); il processore valida e converte in testo separato da virgole
- Testare sempre su collezioni di foto reali prima dell'uso in produzione

## Componenti dell'architettura

### Framework e runtime

- **Axum** - Framework web asincrono moderno costruito su hyper e tower
- **Tokio** - Runtime asincrono per la gestione di richieste concorrenti
- **Monitoraggio basato su polling** - I client interrogano l'endpoint dei risultati del job per aggiornamenti sullo stato di avanzamento

### Strategia di storage

- **Storage basato su filesystem** per tutti i dati (task, foto, job)
- **File JSON** per la persistenza dei metadati
- **File binari** per lo storage delle foto
- **FileSystemLayout** per la gestione centralizzata della struttura delle directory
- **Livello di astrazione** progettato per evoluzione futura (database, object storage)

### Organizzazione basata su catalogo

- Un **Catalog** corrisponde a un catalogo di Lightroom Classic e costituisce l'unità organizzativa di primo livello
- I task e le relative foto sono associati a un catalogo specifico
- L'isolamento dei cataloghi garantisce la completa separazione dei dati tra diversi cataloghi Lightroom

### Supporto multi-task

- Più task possono coesistere simultaneamente all'interno di un catalogo
- Ogni task mantiene collezioni di foto e code di job indipendenti
- L'isolamento dei task garantisce l'assenza di interferenze tra task grazie a directory separate nel filesystem
- Limitazione attuale: lo storage su filesystem è vincolato dallo spazio disco disponibile
- Miglioramento futuro: limiti configurabili su numero di task, storage per-task e pulizia basata su TTL

### Modello di concorrenza

- **Elaborazione asincrona basata su task** utilizzando i task di Tokio
- **Worker pool** con limiti basati sulle GPU (un worker per GPU)
- **Esecuzione basata su coda** per la pianificazione dei job tramite `PhotoBuffer` condiviso

## Concetti principali

### Catalog

Un **Catalog** rappresenta un catalogo di Lightroom Classic. È il contenitore di primo livello nel sistema.

**Caratteristiche:**

- Corrisponde 1:1 a un catalogo di Lightroom Classic
- Identificato da un catalog_id univoco (UUID)
- Contiene tutti i task (e le relative foto/job) per quel catalogo
- Creato implicitamente quando viene creato il primo task per un catalog_id

**Scopo:**

- Garantisce l'isolamento dei dati tra diversi cataloghi Lightroom
- Permette allo stesso server Photometoria di servire più cataloghi

### Task

Un **Task** rappresenta una sessione di lavoro per un fotografo.

**Caratteristiche:**

- Contenitore per foto caricate e suggerimenti contestuali condivisi
- Di breve durata (una sessione di lavoro), ma inizialmente senza timeout automatico
- Le foto rimangono disponibili fino alla cancellazione esplicita del task
- Il contesto può essere modificato dopo la creazione

**Limiti dei task:**

- L'implementazione attuale non impone limiti sul numero di task
- Le versioni future potrebbero introdurre limiti configurabili basati su:
  - Numero totale di task (a livello di sistema o per utente)
  - Applicazione di quote di storage
  - Policy di pulizia basate sul tempo (TTL)
- L'architettura supporta l'aggiunta di questi vincoli senza refactoring significativi

**Ciclo di vita:**

```
Creato → Foto caricate → Job creati/eseguiti → Cancellato esplicitamente
```

### Photo

Una **Photo** è un file immagine caricato per l'analisi.

**Caratteristiche:**

- Appartiene a esattamente un task
- Salvata su filesystem (con quota di storage configurabile)
- Identificata da un photo_id univoco (UUID)
- Contiene metadati: nome file originale, dimensione, timestamp di caricamento

**Vincoli:**

- Non può essere cancellata se referenziata da un job attivo
- Cancellata automaticamente quando il task padre viene eliminato

### Job

Un **Job** è un processo di analisi AI che viene eseguito sulle foto all'interno di un task.

**Caratteristiche:**

- Fa riferimento a un task specifico
- Specifica quale modello AI utilizzare
- Può elaborare tutte le foto nel task o un sottoinsieme specifico
- Lavora su uno snapshot delle foto disponibili al momento della creazione
- Più job possono essere accodati (elaborati concorrentemente fino a uno per GPU)

**Stati:**

- `queued` - In attesa di essere preso in carico da un worker
- `processing` - Attualmente in esecuzione su un worker
- `completed` - Completato con successo
- `failed` - Errore fatale riscontrato
- `cancelled` - Fermato manualmente dall'utente

**Risultati:**

- Disponibili incrementalmente durante l'elaborazione (risultati parziali)
- Rimangono disponibili dopo il completamento fino alla cancellazione del job
- Ogni foto nel job ha uno stato individuale (completed/failed)

**Ciclo di vita:**

```
Creato → In coda → In elaborazione → Completato/Fallito/Cancellato → Eliminato
```

### Worker Pool

Il **Worker Pool** gestisce l'esecuzione concorrente dei job in base alle risorse GPU disponibili.

**Design:**

- Un worker per GPU (configurato nelle impostazioni)
- I worker prelevano singole foto da un `PhotoBuffer` condiviso
- I job passano da `queued` a `processing` non appena un worker prende in carico la prima foto, *prima* che inizi la chiamata di analisi AI — così lo stato riflette il lavoro in corso durante l'intera analisi, non solo dopo il suo completamento
- Ogni worker elabora le foto sequenzialmente

**Configurazione:**

Configurato tramite `OllamaProviderConfig`:

```toml
[ai.providers.ollama]
base_url = "http://localhost:11434"
devices = [0, 1]     # Indici GPU — un worker viene creato per ogni dispositivo
```

Un worker viene creato per ogni GPU elencata in `devices`. Se `devices` è vuoto, viene utilizzato un singolo worker sul dispositivo 0.

---

## Organizzazione dei moduli

La codebase segue una struttura modulare pulita:

```
src/
├── main.rs              # Punto di ingresso dell'applicazione
├── lib.rs               # Esportazioni della libreria per test di integrazione
├── cli.rs               # Definizioni argomenti CLI (clap)
├── startup.rs           # Inizializzazione e avvio del server
├── app_state.rs         # Stato condiviso dell'applicazione (AppState)
├── config/              # Caricamento e tipi di configurazione
│   ├── mod.rs           # Struct Config, load_config()
│   ├── ai.rs            # AIConfig, ProviderConfig, OllamaProviderConfig
│   ├── server.rs        # ServerConfig (host, port)
│   ├── storage.rs       # StorageConfig (percorsi, dimensione massima)
│   ├── upload.rs        # UploadConfig (dimensione massima foto, massimo per richiesta)
│   ├── worker_pool.rs   # WorkerPoolConfig
│   └── byte_size.rs     # Tipo helper ByteSize
├── routes/              # Definizioni degli endpoint REST (routing)
│   └── mod.rs           # create_router() — tutte le mappature delle route
├── handlers/            # Logica di business per ogni endpoint
│   ├── mod.rs
│   ├── tasks.rs         # CRUD task
│   ├── photos.rs        # Get/delete/list foto
│   ├── upload_photos.rs # Gestione upload multipart
│   ├── jobs.rs          # CRUD job + cancel/retry/results
│   ├── providers.rs     # Elenco provider, discovery dei modelli
│   ├── info.rs          # Endpoint informazioni server
│   ├── app_error.rs     # Mappatura AppError → risposte HTTP
│   └── test_utils.rs    # Fixture e helper per i test
├── models/              # Strutture dati
│   ├── mod.rs
│   ├── catalog.rs       # Entità Catalog
│   ├── task.rs          # Entità Task e DTO
│   ├── photo.rs         # Entità Photo e DTO
│   ├── job.rs           # Entità Job e DTO
│   └── info.rs          # Struct di risposta ServerInfo
├── services/            # Integrazioni esterne
│   ├── mod.rs
│   ├── ai/              # Livello di astrazione dei provider AI
│   │   ├── mod.rs       # Esportazioni del modulo
│   │   ├── error.rs     # Tipi AIProviderError
│   │   ├── provider.rs  # Trait AIProvider e tipi comuni
│   │   ├── registry.rs  # ProviderRegistry per la gestione dei provider
│   │   └── ollama/      # Implementazione del provider Ollama
│   │       ├── mod.rs
│   │       ├── provider.rs  # OllamaProvider
│   │       └── types.rs     # Tipi API Ollama
│   └── worker/          # Implementazione del worker pool
│       ├── mod.rs
│       ├── pool.rs      # WorkerPool: loop di discovery, recovery all'avvio
│       ├── worker.rs    # Worker: scheduling con soglia ibrida
│       ├── processor.rs # PhotoProcessor: chiamata AI, aggiornamento stato job
│       └── queue.rs     # PhotoBuffer: coda foto condivisa
└── storage/             # Livello di astrazione per la persistenza
    ├── mod.rs           # Trait degli store + re-export
    ├── task_store.rs    # Trait TaskStore
    ├── photo_store.rs   # Trait PhotoStore
    ├── job_store.rs     # Trait JobStore
    ├── filesystem_task_store.rs
    ├── filesystem_photo_store.rs
    ├── filesystem_job_store.rs
    └── filesystem_layout.rs
```

### Dipendenze principali

- `axum` - Framework web asincrono
- `tokio` - Runtime asincrono
- `reqwest` - Client HTTP per chiamate API Ollama
- `serde` / `serde_json` - Serializzazione JSON
- `uuid` - Generazione di identificatori univoci
- `anyhow` / `thiserror` - Gestione degli errori
- `tracing` - Logging e tracing
- `tower` - Middleware
- `tower-http` - Middleware HTTP (CORS, tracing, ecc.)
- `base64` - Encoding delle immagini per i provider AI
- `async-trait` - Supporto trait asincroni

### Astrazione dei provider AI

Il sistema utilizza un livello di astrazione dei provider (`services/ai/`) per supportare più backend AI:

**Componenti principali:**

- **Trait `AIProvider`** - Interfaccia comune per tutti i provider AI
  - `check_health()` - Verifica la disponibilità del provider
  - `list_models()` - Ottieni i modelli disponibili
  - `analyze_image()` - Esegui l'analisi dell'immagine

- **`ProviderRegistry`** - Gestisce le istanze dei provider
  - Memorizza i provider per nome
  - Fornisce accesso al provider predefinito
  - Creato dalla configurazione all'avvio

- **`OllamaProvider`** - Implementazione Ollama
  - Chiama l'API REST di Ollama (`/api/tags`, `/api/generate`)
  - Supporta modelli vision (llava, qwen3-vl, ecc.)
  - Timeout e mappature dei modelli configurabili

**Vantaggi del design:**

- **Estensibilità** - Aggiungere nuovi provider senza modificare gli handler
- **Testabilità** - Mock dei provider per unit test (WireMock per integrazione)
- **Guidato dalla configurazione** - Selezione dei provider tramite TOML
- **A prova di futuro** - Pronto per OpenAI, Anthropic e altri provider

## Strategia di implementazione

### Livelli di astrazione

L'implementazione utilizza l'astrazione per consentire l'evoluzione futura senza refactoring significativi:

**TaskStore**

- Interfaccia: astrazione basata su trait (trait `TaskStore`)
- Attuale: `FileSystemTaskStore` con persistenza basata su JSON
- Futuro: supportato da database (PostgreSQL, SQLite), cache Redis o approcci ibridi

**JobStore**

- Attuale: `FileSystemJobStore` con file JSON per job
- Futuro: PostgreSQL, SQLite o altro database

**PhotoStore**

- Attuale: `FileSystemPhotoStore` con dati binari delle foto e metadati JSON
- Futuro: object storage (S3, MinIO), metadati supportati da database

**FileSystemLayout**

- Gestione centralizzata della struttura delle directory
- Generazione consistente dei percorsi in tutte le implementazioni di storage
- Struttura delle directory: `{storage_path}/catalogs/{catalog_id}/tasks/{task_id}/` con sottodirectory per foto (`imgs/`) e job

**TaskQueue**

- Attuale: `VecDeque` in memoria con `Mutex`
- Futuro: Redis, RabbitMQ o altra coda di messaggi

**NotificationManager**

- Attuale: SSE con tracciamento delle connessioni in memoria
- Futuro: WebSocket o sistema pub/sub esterno

### Astrazione dello storage

Il livello di storage utilizza pattern di astrazione basati su trait per consentire l'evoluzione futura.

**Design pattern:**

- I trait `TaskStore`, `PhotoStore` e `JobStore` definiscono le interfacce di storage
- Il design basato su trait consente implementazioni multiple senza modificare la logica di business
- Tutti i metodi sono asincroni e restituiscono `Result<T, StoreError>` per una corretta gestione degli errori
- Operazioni thread-safe (vincoli `Send + Sync`) per accesso concorrente da più task Tokio

**Implementazione attuale: basata su filesystem**

- **FileSystemTaskStore**: file JSON per task (`task.json`)
- **FileSystemPhotoStore**: dati binari delle foto nella sottodirectory `imgs/`, metadati in `photos.json`
- **FileSystemJobStore**: file JSON per job nella sottodirectory `jobs/`
- **FileSystemLayout**: generazione centralizzata dei percorsi e gestione della struttura delle directory
- I dati persistono tra i riavvii del server
- Utilizza I/O asincrono su file di Tokio per operazioni non bloccanti
- Accesso concorrente gestito tramite lock asincroni sui file
- Adatto per deployment su singolo server

**Struttura delle directory:**

```text
{storage_path}/
└── catalogs/
    └── {catalog_id}/
        ├── catalog.json           # Metadati del catalogo
        └── tasks/
            └── {task_id}/
                ├── task.json          # Metadati del task
                ├── photos.json        # Metadati delle foto
                ├── imgs/              # Dati binari delle foto
                │   ├── {photo_id_1}
                │   └── {photo_id_2}
                └── jobs/              # Metadati dei job
                    ├── {job_id_1}.json
                    └── {job_id_2}.json
```

**Implementazioni future:**

- **Supportato da database**: PostgreSQL o SQLite per query e indicizzazione migliori
- **Object storage**: S3, MinIO o simili per lo storage delle foto
- **Redis**: cache distribuita con supporto TTL per pulizia automatica
- **Ibrido**: metadati su database + object storage per le foto
- **Limiti personalizzati**: applicazione di quote, eviction LRU, isolamento per utente

**Thread safety:**

Tutte le implementazioni di storage devono essere `Send + Sync` e supportare l'accesso concorrente da più task Tokio senza data race. Il design basato su trait garantisce che questo contratto sia applicato a tempo di compilazione.

### Implementazione del Worker Pool

**Design:**

- Task Tokio per worker (uno per GPU)
- `PhotoBuffer` condiviso con selezione delle foto basata su priorità
- Discovery dei job basata su polling: il pool interroga periodicamente il JobStore per nuovi job in coda
- Scheduling consapevole dei modelli per minimizzare gli swap in VRAM
- Recovery dei job stale all'avvio (i job in stato `processing` vengono reimpostati a `queued`)

**Loop del worker (a livello di foto con soglia ibrida):**

I worker prelevano singole foto dal `PhotoBuffer` condiviso utilizzando una strategia di selezione smart ibrida che bilancia efficienza (minimizzare gli swap di modello) con equità (garantire che tutti i job progrediscano):

```
Loop del worker:
  1. Carica il modello AI se necessario
  2. Inizializza: photos_processed = 0, model_load_time = now()
  3. Loop:
     a. Seleziona prossima foto dal buffer:
        - Sotto le soglie (conteggio OPPURE tempo): dare priorità alle foto con il modello attuale
        - Sopra entrambe le soglie: accettare qualsiasi foto (permettere swap per equità)
     b. Se la foto richiede un modello diverso: carica il nuovo modello, resetta i contatori
     c. Chiama l'API del provider AI (tramite il trait AIProvider)
     d. Salva il risultato incrementalmente
     e. Incrementa photos_processed
     f. Se non ci sono più foto: interrompi
```

Per l'analisi dettagliata dell'algoritmo con soglia ibrida e i relativi trade-off, vedi [Strategia di selezione delle foto](photo-selection-strategy-it.md).

### Deduplicazione delle foto (Futura)

**Design per implementazione futura:**

- Utilizzo dell'UUID della foto per rilevare e prevenire caricamenti duplicati all'interno dello stesso task
- Al caricamento, verificare se la stessa foto (per nome file originale e dimensione) esiste già nel task
- Se viene rilevato un duplicato, restituire il photo_id esistente anziché creare una nuova entry

**Vantaggi:**

- Previene caricamenti duplicati accidentali all'interno di un task
- Risparmia spazio di storage senza aggiungere complessità

**Nota implementativa:**

Non implementato inizialmente per mantenere semplice la prima versione.

## Vedi anche

- [Riferimento API](api-reference.md) - Documentazione completa degli endpoint
- [Configurazione](configuration.md) - Riferimento configurazione del server
- [Guida allo sviluppo](development.md) - Workflow di sviluppo e testing
- [Strategia di selezione delle foto](photo-selection-strategy-it.md) - Algoritmo con soglia ibrida per lo scheduling delle foto nei worker
