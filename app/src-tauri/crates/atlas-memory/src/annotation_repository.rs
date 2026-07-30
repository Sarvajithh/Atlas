//! `AnnotationRepository` interface (§33.8). Implemented by atlas-db.

use atlas_types::ids::{AnnotationId, DocumentId};
use atlas_types::memory::Annotation;
use atlas_utils::AppError;

pub trait AnnotationRepository: Send + Sync {
    fn list_for_document(&self, document_id: DocumentId) -> Result<Vec<Annotation>, AppError>;
    fn insert(&self, annotation: Annotation) -> Result<Annotation, AppError>;
    fn update(&self, annotation: Annotation) -> Result<Annotation, AppError>;
    fn delete(&self, id: AnnotationId) -> Result<(), AppError>;
}
