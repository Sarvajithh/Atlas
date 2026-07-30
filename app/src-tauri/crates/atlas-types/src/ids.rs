//! Newtype identifiers for every entity defined in §33 (Database Schema).
//! Newtypes prevent accidentally passing a `DocumentId` where a `WorkspaceId`
//! is expected, without adding any behavior beyond identity.

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        pub struct $name(pub i64);
    };
}

id_type!(WorkspaceId);
id_type!(DocumentId);
id_type!(ChunkId);
id_type!(ConceptNodeId);
id_type!(ConceptEdgeId);
id_type!(AnnotationId);
id_type!(BookmarkId);
id_type!(ChatSessionId);
id_type!(ChatMessageId);
id_type!(ModelRegistryId);
id_type!(JobId);
id_type!(EventId);
id_type!(RevisionHistoryId);
