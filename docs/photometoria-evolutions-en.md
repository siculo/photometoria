# Photometoria — Possible Evolutions

Summary of the discussion about future directions for the project, explored in February 2026.

## Current State

The project has completed its first milestone: the main REST API services are implemented, and storage has been migrated from in-memory to persistent JSON file-based storage. The repository is at https://github.com/siculo/photometoria.

The current system analyzes individual photos through vision models (qwen2-vl:8b via Ollama) and generates descriptive tags, also considering the user-provided context in the Task.

---

## Direction 1: Analysis Beyond the Single Photo

The core idea is to move past independent analysis of each photo toward building relationships between photos. This breaks down along three axes:

**Subject (who):** requires cross-photo face recognition. It's not enough to recognize that a photo contains a face — the same face needs to be linked across different photos and associated with an identity (with user intervention).

**Place (where):** already partially achievable through two sources: visual tagging (the model recognizes a restaurant, a landmark) and GPS coordinates from EXIF data. The two are complementary — GPS provides the "precise where", the model provides the "semantic where".

**Event (what):** the most elusive concept, as it emerges from the intersection of other data. "Marco's birthday" isn't visible in a single photo but is reconstructed from the combination of temporal proximity, location, present subjects, and user-provided context.

These three axes define increasing levels of abstraction: **facts** from the individual photo (tags, faces, GPS, date), **relationships** between photos (same subject, same place, temporal proximity), **concepts** that emerge from the combination (events, trips, occasions).

---

## Direction 2: Face Recognition

### Standard Pipeline

Cross-photo face recognition follows a well-established flow:

1. **Face detection** — locate faces in each photo (bounding boxes and facial landmarks)
2. **Face embedding** — for each detected face, generate a numeric vector (128-512 dimensions) representing it in a geometric space
3. **Clustering** — group similar embeddings to identify "this is the same person across N photos" (algorithms like DBSCAN/HDBSCAN)
4. **Labeling** — the user assigns a name to each cluster

### Face Detection Models

- **RetinaFace:** state of the art for accuracy, Feature Pyramid Network architecture, returns bounding boxes + 5 facial landmarks. Ideal for offline batch processing.
- **MTCNN:** cascade of three networks (P-Net, R-Net, O-Net), lighter than RetinaFace but less accurate on difficult cases.
- **MediaPipe (BlazeFace):** Google framework optimized for real-time/mobile, less suited for quality batch analysis.

### Important Note

Face recognition **does not** use the generative vision models already in use (qwen, llava). It requires specialized models, lighter and faster, that produce numeric embeddings. It's a completely separate pipeline from tagging.

### Integration with Photometoria

The hypothesis is to treat face recognition as a new type of provider (e.g. `FaceProvider`), separate from the existing `VisionProvider`. Libraries like **InsightFace** (Python, integrating RetinaFace + ArcFace) could be encapsulated in an HTTP microservice, following the same communication pattern used with Ollama.

The Job concept extends naturally: in addition to tagging jobs, there would be face extraction jobs with different outputs (embeddings and bounding boxes rather than text tags). Clustering and labeling remain Photometoria's responsibility, not the provider's.

---

## Direction 3: Semantic Search

To support natural language queries like "photos from Marco's birthday at the restaurant", two components are needed:

**Structured metadata + traditional search:** tags in a database with indices, facial clusters with photo↔person associations, combinable queries.

**Multimodal embeddings + vector search:** models like CLIP generate an embedding of the entire photo that captures its overall "meaning". This enables natural language searches even without exact tags. Requires a vector database (ChromaDB, Qdrant, pgvector).

**Hybrid approach (recommended):** facial embeddings for people, structured tags for places and characteristics, CLIP embeddings for fuzzy semantic search. The query "photos in Paris with Marco" becomes a combination of filters on person clusters, location tags, and proximity in vector space.

---

## Landscape of Similar Tools

### Self-hosted Complete Solutions
- **Immich** — the most complete: face recognition, CLIP search, mobile backup, multi-user. Microservices architecture (Node.js + Python ML + PostgreSQL). Stable since v2.0 (October 2025).
- **PhotoPrism** — AI tagging and search, written in Go, more focused on web-based library exploration.
- **HomeGallery** — lighter, Node.js, similar image search and face discovery.

### Desktop Open Source
- **digiKam** — KDE veteran, face recognition, batch processing, full EXIF/IPTC/XMP support, handles libraries with over 100k images.
- **Shotwell** — simple, no AI, date-based organization.

### Tagging Tools
- **STAG** — Apache 2.0, born from a need similar to Photometoria's: local batch tagging with an ML model (recognize-anything), output to XMP sidecar files. Standalone tool, not an API service.
- **OpenPhotos** — local Google Photos alternative with face recognition (DeepFace) and Ollama. Conceptually close to Photometoria.

### Commercial/Enterprise
- **Adobe Experience Manager Smart Tagging** — AI trainable on business vocabulary, enterprise-grade.
- **Daminion** — commercial DAM with face recognition and identity propagation.
- **CYME, ioMoVo, PhotoTag.ai, Auto Metadata AI** — cloud services for stock photography and enterprise DAM.

### Photometoria's Positioning
Photometoria is not a gallery (Immich/PhotoPrism), not an enterprise DAM, not a standalone tool (STAG). It is a **photo analysis API service** designed to integrate with existing workflows (Lightroom and others), with support for multiple AI providers and local processing. This nature as an integrable service is a potential strength.

---

## Deployment Considerations

Photometoria's target audience includes photographers (both professionals and enthusiasts) who generally aren't technical. Setup complexity is the main barrier to adoption.

**Docker Compose** is the most realistic path: a single file orchestrating all components (Rust server, Ollama with models, any Python services, database). This is the pattern adopted by Immich, PhotoPrism, and HomeGallery.

An **opinionated default configuration** is important: a preconfigured stack that works out of the box (e.g. Ollama + a specific model), with advanced configuration available for those who know what they're doing.

Each additional provider (Python service for face recognition, vector database for semantic search) adds deployment complexity that needs to be managed.

---

## Nature of the Project

Photometoria is an open source project (Apache 2.0) born to satisfy a personal need, out of curiosity, to gain experience and learn. It is not a commercial initiative. Future directions will grow at the pace of the author's curiosity and interest, which is consistent with the project's nature.

---

*Document generated on February 13, 2026*
