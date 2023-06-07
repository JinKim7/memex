use rust_bert::{
    pipelines::sentence_embeddings::{SentenceEmbeddingsBuilder, SentenceEmbeddingsModelType},
    RustBertError,
};
use std::{sync::mpsc, thread::JoinHandle};
use thiserror::Error;
use tokenizers::{Tokenizer, TruncationParams};
use tokio::{sync::oneshot, task};

#[derive(Debug, Error)]
pub enum EmbeddingError {
    #[error("Failed to encode string: {0}")]
    EncodingFailure(String),
    #[error("Unable to load model: {0}")]
    SetupError(String),
}

#[derive(Clone, Copy, Debug)]
pub enum EmbeddingsModelType {
    DistiluseBaseMultilingualCased,
    BertBaseNliMeanTokens,
    AllMiniLmL12V2,
    AllMiniLmL6V2,
    AllDistilrobertaV1,
    ParaphraseAlbertSmallV2,
    SentenceT5Base,
}

impl From<EmbeddingsModelType> for SentenceEmbeddingsModelType {
    fn from(val: EmbeddingsModelType) -> Self {
        match val {
            EmbeddingsModelType::AllDistilrobertaV1 => {
                SentenceEmbeddingsModelType::AllDistilrobertaV1
            }
            EmbeddingsModelType::AllMiniLmL12V2 => SentenceEmbeddingsModelType::AllMiniLmL12V2,
            EmbeddingsModelType::AllMiniLmL6V2 => SentenceEmbeddingsModelType::AllMiniLmL6V2,
            EmbeddingsModelType::BertBaseNliMeanTokens => {
                SentenceEmbeddingsModelType::BertBaseNliMeanTokens
            }
            EmbeddingsModelType::DistiluseBaseMultilingualCased => {
                SentenceEmbeddingsModelType::DistiluseBaseMultilingualCased
            }
            EmbeddingsModelType::ParaphraseAlbertSmallV2 => {
                SentenceEmbeddingsModelType::ParaphraseAlbertSmallV2
            }
            EmbeddingsModelType::SentenceT5Base => SentenceEmbeddingsModelType::SentenceT5Base,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ModelConfig {
    model: EmbeddingsModelType,
    max_length: usize,
    stride: usize,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            model: EmbeddingsModelType::AllMiniLmL12V2,
            max_length: 256,
            stride: 128,
        }
    }
}

type Message = (String, oneshot::Sender<Vec<Vec<f32>>>);

#[derive(Debug)]
pub struct SentenceEmbedder {
    sender: mpsc::SyncSender<Message>,
}

impl SentenceEmbedder {
    /// spawn the embedder on a separate thread.
    pub fn spawn(
        model_config: &ModelConfig,
    ) -> (JoinHandle<Result<(), RustBertError>>, SentenceEmbedder) {
        let (sender, receiver) = mpsc::sync_channel(100);
        let model_config = model_config.to_owned();
        let handle = std::thread::spawn(move || Self::runner(receiver, model_config));
        (handle, SentenceEmbedder { sender })
    }

    /// The sentence embedding runner itself
    fn runner(
        receiver: mpsc::Receiver<Message>,
        model_config: ModelConfig,
    ) -> Result<(), RustBertError> {
        // Needs to be in sync runtime, async doesn't work
        let model: rust_bert::pipelines::sentence_embeddings::SentenceEmbeddingsModel =
            SentenceEmbeddingsBuilder::remote(model_config.model.into()).create_model()?;

        while let Ok((text, sender)) = receiver.recv() {
            let segments = segment_text(&model_config, &text).unwrap();

            let results = model.encode(&segments)?;
            sender.send(results).expect("sending results");
        }

        Ok(())
    }

    /// Encode the sentences and return the results
    pub async fn encode(&self, text: String) -> anyhow::Result<Vec<Vec<f32>>> {
        let (sender, receiver) = oneshot::channel();
        task::block_in_place(|| self.sender.send((text, sender)))?;
        Ok(receiver.await?)
    }
}

/// Segment a doc into the proper windowed
pub fn segment_text(model_config: &ModelConfig, text: &str) -> Result<Vec<String>, EmbeddingError> {
    let model_name = match model_config.model {
        EmbeddingsModelType::AllMiniLmL12V2 => "sentence-transformers/all-MiniLM-L12-v2",
        EmbeddingsModelType::AllMiniLmL6V2 => "sentence-transformers/all-MiniLM-L6-v2",
        EmbeddingsModelType::AllDistilrobertaV1 => "sentence-transformers/all-distilroberta-v1",
        _ => return Err(EmbeddingError::SetupError("Model not supported yet".into())),
    };

    let mut tokenizer = match Tokenizer::from_pretrained(model_name, None) {
        Ok(tokenizer) => tokenizer,
        Err(_) => {
            return Err(EmbeddingError::SetupError(format!(
                "Unable to load model <{}>",
                model_name
            )))
        }
    };

    tokenizer.with_truncation(Some(TruncationParams {
        max_length: model_config.max_length,
        stride: model_config.stride,
        ..Default::default()
    }));

    let mut segments = Vec::new();

    let encoding = tokenizer.encode(text, false).unwrap();
    let decoded = match tokenizer.decode(encoding.get_ids().to_vec(), false) {
        Ok(decoded) => decoded,
        Err(_) => return Err(EmbeddingError::EncodingFailure(text.to_string())),
    };

    segments.push(decoded);
    for encoding in encoding.get_overflowing() {
        let decoded = match tokenizer.decode(encoding.get_ids().to_vec(), false) {
            Ok(decoded) => decoded,
            Err(_) => return Err(EmbeddingError::EncodingFailure(text.to_string())),
        };

        segments.push(decoded);
    }

    Ok(segments)
}

#[cfg(test)]
mod test {
    use rust_bert::pipelines::sentence_embeddings::{
        SentenceEmbeddingsBuilder, SentenceEmbeddingsModelType,
    };
    use tokenizers::{Tokenizer, TruncationParams};

    #[test]
    fn test_tokenizer() {
        let model = SentenceEmbeddingsBuilder::remote(SentenceEmbeddingsModelType::AllMiniLmL12V2)
            .create_model()
            .unwrap();

        let string = include_str!("test.txt");
        let mut tokenizer =
            Tokenizer::from_pretrained("sentence-transformers/all-MiniLM-L12-v2", None).unwrap();
        tokenizer.with_truncation(Some(TruncationParams {
            max_length: 256,
            stride: 128,
            ..Default::default()
        }));

        let encoding = tokenizer.encode(string, false).unwrap();
        let encode_one = model.encode(&[string]).unwrap();

        let test = encoding.get_ids();

        let decoded = tokenizer
            .decode(encoding.get_ids().to_vec(), false)
            .unwrap();
        let encode_two = model.encode(&[decoded.clone()]).unwrap();
        assert_eq!(
            test,
            tokenizer.encode(decoded.clone(), false).unwrap().get_ids()
        );
        assert_eq!(encode_one, encode_two);

        println!("{:?}", decoded);
        for (idx, encoding) in encoding.get_overflowing().iter().enumerate() {
            let decoded = tokenizer.decode(encoding.get_ids().to_vec(), true);
            println!("{} - {:?} - {:?}", idx, encoding.len(), decoded);
        }

        assert_eq!(encoding.len(), 0);
    }
}
