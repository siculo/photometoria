<!-- translation-source: photo-selection-strategy.md @ 96e7fe4f656ee757136800f432ac5c78e0c685d5 -->

> **Nota:** Questa è una traduzione della [versione inglese](photo-selection-strategy.md).
> In caso di discrepanze, fa fede l'originale.

# Strategia di selezione delle foto (Work in Progress)

Nell'implementazione del worker pool, una decisione architetturale chiave riguarda il modo in cui i worker selezionano la prossima foto da elaborare. Questo è particolarmente importante quando più job utilizzano modelli AI diversi.

## La sfida: Efficienza vs. Equità

Con più job che utilizzano modelli diversi e GPU limitate, c'è un trade-off fondamentale:

**Esecuzione sequenziale dei job:**
- ✅ Swap di modello minimi (un caricamento per job)
- ✅ Massima efficienza (~6% di overhead)
- ❌ Equità scarsa (i job attendono in coda fino al completamento degli altri)

**Selezione round-robin delle foto:**
- ✅ Equità perfetta (tutti i job progrediscono simultaneamente)
- ❌ Swap di modello eccessivi (uno per foto se i modelli differiscono)
- ❌ Overhead severo (~77% nel caso peggiore)

## Selezione smart basata su priorità con soglia ibrida

Una strategia ibrida che bilancia efficienza ed equità utilizzando vincoli sia di conteggio che temporali:

**Algoritmo:**

1. Il worker tiene traccia del modello caricato, del contatore di foto e del tempo trascorso dal caricamento del modello
2. Quando seleziona la foto successiva:
   - Se `contatore < min_photos` OPPURE `tempo_trascorso < max_time`: Dare priorità alle foto che richiedono il modello attuale
   - Se `contatore >= min_photos` E `tempo_trascorso >= max_time`: Accettare qualsiasi foto (permettere lo swap del modello)
3. Resettare contatore e timer quando il modello cambia

**Perché ibrido (Conteggio + Tempo)?**

- **Solo soglia di conteggio**: Iniquo quando le foto hanno complessità diverse (foto grandi vs. piccole)
- **Solo soglia temporale**: Potrebbe effettuare swap troppo presto con foto molto veloci (ammortizzazione insufficiente dell'overhead di caricamento del modello)
- **Ibrido**: Garantisce sia ammortizzazione minima (conteggio) che equità temporale (tempo)

**Pseudo-codice:**

```rust
fn select_next_photo(&mut self, min_photos: usize, max_time: Duration) -> Option<Photo> {
    let current_model = self.loaded_model;
    let counter = self.photos_processed;
    let elapsed = self.model_load_time.elapsed();

    // Sotto le soglie: preferire lo stesso modello (evitare swap)
    if counter < min_photos || elapsed < max_time {
        if let Some(photo) = find_photo_with_model(current_model) {
            return Some(photo);
        }
    }

    // Sopra entrambe le soglie: accettare qualsiasi foto (permettere swap per equità)
    find_any_available_photo()
}
```

## Analisi delle prestazioni

**Scenario:** 1 GPU, 2 job (50 foto ciascuno), modelli diversi (qwen3.5, llava)

| Strategia | Tempo | Overhead | Primo risultato Job B | Equità |
|-----------|-------|----------|----------------------|--------|
| Sequenziale (soglia=∞) | 320s | 20s (6%) | t=170s | Scarsa |
| Smart (soglia=25) | 340s | 40s (12%) | t=95s | Buona |
| Smart (soglia=10) | 420s | 100s (24%) | t=50s | Eccellente |
| Round-robin (soglia=1) | 1300s | 1000s (77%) | t=23s | Perfetta |

**Risultati chiave:**

- **Soglia=20-25 foto**: Miglior bilanciamento per l'uso in produzione (~6-12% overhead, buona equità)
- **Soglia=50+ foto**: Equivalente all'esecuzione sequenziale dei job (massima efficienza, scarsa equità)
- **Soglia=1-5 foto**: Comportamento quasi round-robin (scarsa efficienza, massima equità)

*Nota: l'analisi sopra utilizza soglie basate sul conteggio per semplicità. L'approccio ibrido (conteggio + tempo) fornisce un'equità superiore come spiegato di seguito.*

## Soglia basata sul tempo vs. sul conteggio vs. ibrida

**Il problema con la soglia solo a conteggio:**

Quando le foto hanno complessità di elaborazione diverse, le soglie basate sul conteggio portano a iniquità temporale:

```
Scenario: 1 GPU, 2 job, soglia conteggio = 20 foto

Job A: 50 foto ad alta risoluzione (5s ciascuna)
Job B: 50 foto a bassa risoluzione (1s ciascuna)

Ciclo 1:
  Job A: 20 foto × 5s = 100s di tempo GPU
  Job B: 20 foto × 1s = 20s di tempo GPU

Ciclo 2:
  Job A: 20 foto × 5s = 100s di tempo GPU
  Job B: 20 foto × 1s = 20s di tempo GPU

Risultato:
  ❌ Job A ottiene 5× più tempo GPU
  ❌ Iniquità temporale
```

**Soglia solo temporale:**

Fornisce equità temporale ma potrebbe effettuare swap troppo presto:

```
Soglia temporale = 60s

Job con foto molto veloci (0.5s ciascuna):
  - Elabora 120 foto in 60s
  - Overhead caricamento modello (10s) ben ammortizzato ✓

Job con foto ultra-veloci (0.1s ciascuna):
  - Elabora 600 foto in 60s
  - Ma potrebbe elaborare 100 foto (10s) poi effettuare swap
  - Overhead caricamento modello non completamente ammortizzato ⚠️
```

**Soglia ibrida (Raccomandata):**

Combina entrambi i vincoli per un comportamento ottimale:

```
min_photos = 10, max_time = 120s

Job A (alta risoluzione, 5s/foto):
  - Elabora 10 foto (50s) → min_photos ✓, tempo < 120s → continua
  - Elabora altre 14 foto (70s) → 24 totali, 120s raggiunti → swap
  - Risultato: 24 foto, 120s di tempo GPU

Job B (bassa risoluzione, 1s/foto):
  - Elabora 10 foto (10s) → min_photos ✓, tempo < 120s → continua
  - Elabora altre 110 foto (110s) → 120 totali, 120s raggiunti → swap
  - Risultato: 120 foto, 120s di tempo GPU

✅ Equità temporale (entrambi ottengono 120s)
✅ Overhead caricamento modello ben ammortizzato (minimo 10 foto)
✅ Si adatta automaticamente alla complessità delle foto
```

**Confronto:**

| Tipo di soglia | Equità temporale | Protezione dall'overhead | Complessità |
|----------------|------------------|--------------------------|-------------|
| Solo conteggio | ❌ Scarsa (varia con la complessità delle foto) | ✅ Buona | Bassa |
| Solo tempo | ✅ Eccellente | ⚠️ Moderata (potrebbe effettuare swap troppo presto) | Media |
| **Ibrida** | **✅ Eccellente** | **✅ Eccellente** | **Media** |

## Vantaggi dell'approccio ibrido

**1. Equità temporale:**
- Ogni job riceve approssimativamente lo stesso tempo GPU, indipendentemente dalla complessità delle foto
- Impedisce ai job con foto complesse di monopolizzare le risorse GPU
- Prevedibile: "Ogni job progredisce ogni 2 minuti" (anziché "ogni N foto")
- Migliore per scenari multi-utente e garanzie QoS/SLA

**2. Protezione dall'overhead:**
- `min_photos` garantisce che l'overhead di caricamento del modello sia ben ammortizzato
- Non effettuerà swap dopo solo 1-2 foto anche se la soglia temporale è bassa
- Protegge da casi patologici (foto piccole ultra-veloci)

**3. Trade-off configurabile:**
- Sintonizzazione su entrambe le dimensioni in base alle caratteristiche del carico di lavoro
- Soglie alte per carichi di lavoro dove l'efficienza è critica
- Soglie basse per scenari interattivi rivolti all'utente
- Esempio: `min_photos=10, max_time=60s` per UI reattiva, `min_photos=50, max_time=300s` per elaborazione batch

**4. Comportamento adattivo:**
- Se tutti i job usano lo stesso modello: zero overhead (nessuno swap)
- Se un job termina in anticipo: continua con il job rimanente senza swap non necessari
- Automaticamente ottimale per carichi di lavoro omogenei
- Si adatta alla complessità delle foto senza configurazione manuale

**5. Migliore esperienza utente:**

```
Esecuzione sequenziale:
  Job A: ████████████████ (completa, poi parte Job B)
  Job B: ................ ████████████████

Priorità smart ibrida (min=10, max=120s):
  Job A: ███...███...███...███...███
  Job B: ...███...███...███...███...███

Entrambi i job mostrano progressi simultaneamente!
Equità temporale: ciascuno ottiene ~120s per ciclo
```

**6. Località del modello:**
- Sfrutta la funzionalità keep-alive di Ollama (i modelli rimangono in VRAM per 5 minuti di default)
- Elabora più foto con lo stesso modello prima di effettuare swap
- Minimizza le costose operazioni di caricamento dei modelli (10-20s per caricamento)

## Considerazioni implementative

**Struttura della coda:**

```rust
struct PhotoQueue {
    // Foto organizzate per modello richiesto
    photos_by_model: HashMap<ModelId, VecDeque<PhotoId>>,

    // Tutte le foto in attesa (per overflow della soglia)
    all_photos: VecDeque<PhotoId>,
}

struct Worker {
    // Stato del modello attuale
    current_model: ModelId,
    photos_processed: usize,
    model_load_time: Instant,

    // Configurazione
    min_photos_before_swap: usize,
    max_time_before_swap: Duration,
}

impl Worker {
    fn should_allow_model_swap(&self) -> bool {
        self.photos_processed >= self.min_photos_before_swap
            && self.model_load_time.elapsed() >= self.max_time_before_swap
    }
}
```

**Configurazione:**

```toml
[worker_pool]
# Numero minimo di foto da elaborare prima di permettere lo swap del modello (protezione dall'overhead)
min_photos_before_swap = 10

# Tempo massimo con lo stesso modello prima di forzare lo swap (equità temporale)
# Formato: stringa di durata (es. "60s", "2m", "120s")
max_time_before_swap = "120s"
```

**Valori raccomandati:**

| Caso d'uso | min_photos | max_time | Motivazione |
|------------|------------|----------|-------------|
| **UI interattiva** (default) | 10 | 60-120s | Feedback veloce, buona equità |
| **Elaborazione batch** | 50 | 300s (5m) | Maggiore efficienza, meno necessità di equità |
| **Multi-tenant/SLA** | 5 | 30-60s | Garanzie di equità rigorose |

**Metriche da monitorare:**
- Swap di modello per job
- Tempo speso nel caricamento del modello vs. elaborazione
- Metrica di equità: deviazione standard del tempo GPU per job
- Foto elaborate per swap di modello (efficienza di ammortizzazione)
- Tempo al primo risultato per job (reattività)

## Raccomandazione

Per l'implementazione iniziale, utilizzare il **profilo UI interattiva**:
```toml
min_photos_before_swap = 10
max_time_before_swap = "120s"
```

**Perché questi valori:**
- ✅ Overhead caricamento modello (10s) ammortizzato su 10+ foto (10% o meno di overhead)
- ✅ Equità temporale: tutti i job mostrano progressi ogni ~2 minuti
- ✅ Buona esperienza utente: aggiornamenti polling reattivi
- ✅ Funziona bene per carichi di lavoro fotografici tipici (20-200 foto per job)
- ✅ Si adatta automaticamente alla complessità delle foto senza necessità di configurazione

Entrambe le soglie dovrebbero essere esposte come opzioni di configurazione per permettere agli utenti di regolarle in base alle proprie esigenze specifiche e all'hardware.
