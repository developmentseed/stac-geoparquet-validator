#![allow(dead_code)]

use arrow::{
    array::{
        ArrayRef, BinaryArray, Float64Array, ListBuilder, RecordBatch, StringArray, StringBuilder,
        StructArray, TimestampMicrosecondArray,
    },
    datatypes::{DataType, Field, Fields, Schema, TimeUnit},
};
use bytes::Bytes;
use parquet::{
    arrow::ArrowWriter,
    basic::{Compression, ZstdLevel},
    file::{metadata::KeyValue, properties::WriterProperties},
};
use serde_json::{Value, json};
use std::sync::Arc;

fn utf8_list_field() -> Field {
    Field::new("item", DataType::Utf8, true)
}

fn bbox_fields() -> Fields {
    ["xmin", "ymin", "xmax", "ymax"]
        .into_iter()
        .map(|name| Field::new(name, DataType::Float64, true))
        .collect()
}

fn link_fields() -> Fields {
    ["href", "rel", "type", "title"]
        .into_iter()
        .map(|name| Field::new(name, DataType::Utf8, true))
        .collect()
}

fn data_asset_fields() -> Fields {
    Fields::from(vec![
        Field::new("href", DataType::Utf8, true),
        Field::new("type", DataType::Utf8, true),
        Field::new("roles", DataType::List(Arc::new(utf8_list_field())), true),
    ])
}

fn wkb_point(x: f64, y: f64) -> Vec<u8> {
    let mut bytes = vec![1u8, 1, 0, 0, 0];
    bytes.extend_from_slice(&x.to_le_bytes());
    bytes.extend_from_slice(&y.to_le_bytes());
    bytes
}

fn string_lists(n: usize, values: &[&str]) -> ArrayRef {
    let mut builder =
        ListBuilder::new(StringBuilder::new()).with_field(Arc::new(utf8_list_field()));
    for _ in 0..n {
        for value in values {
            builder.values().append_value(value);
        }
        builder.append(true);
    }
    Arc::new(builder.finish())
}

/// Builds a valid stac-geoparquet batch of `n` items.
pub fn stac_batch(n: usize) -> RecordBatch {
    let xs: Vec<f64> = (0..n)
        .map(|i| -180.0 + 360.0 * (i as f64) / (n.max(1) as f64))
        .collect();
    let ys: Vec<f64> = (0..n).map(|i| (i % 10) as f64).collect();

    let ids = StringArray::from_iter_values((0..n).map(|i| format!("item-{i:06}")));
    let geometry =
        BinaryArray::from_iter_values(xs.iter().zip(&ys).map(|(x, y)| wkb_point(*x, *y)));
    let bbox = StructArray::new(
        bbox_fields(),
        vec![
            Arc::new(Float64Array::from(xs.clone())) as ArrayRef,
            Arc::new(Float64Array::from(ys.clone())),
            Arc::new(Float64Array::from(xs)),
            Arc::new(Float64Array::from(ys)),
        ],
        None,
    );

    let link_values = StructArray::new(
        link_fields(),
        link_fields()
            .iter()
            .map(|_| Arc::new(StringArray::from(vec!["x"; n])) as ArrayRef)
            .collect(),
        None,
    );
    let links = arrow::array::ListArray::new(
        Arc::new(Field::new("item", DataType::Struct(link_fields()), true)),
        arrow::buffer::OffsetBuffer::from_lengths(vec![1; n]),
        Arc::new(link_values),
        None,
    );

    let data = StructArray::new(
        data_asset_fields(),
        vec![
            Arc::new(StringArray::from(vec!["https://example.com/data.tif"; n])) as ArrayRef,
            Arc::new(StringArray::from(vec!["image/tiff"; n])),
            string_lists(n, &["data"]),
        ],
        None,
    );
    let assets = StructArray::new(
        Fields::from(vec![Field::new(
            "data",
            DataType::Struct(data_asset_fields()),
            true,
        )]),
        vec![Arc::new(data) as ArrayRef],
        None,
    );

    let collection = StringArray::from(vec!["test"; n]);
    let datetime = TimestampMicrosecondArray::from_iter_values(
        (0..n as i64).map(|i| 1_700_000_000_000_000 + i * 1_000_000),
    )
    .with_timezone("UTC");

    let schema = Schema::new(vec![
        Field::new(
            "stac_extensions",
            DataType::List(Arc::new(utf8_list_field())),
            true,
        ),
        Field::new("id", DataType::Utf8, true),
        Field::new("geometry", DataType::Binary, true),
        Field::new("bbox", DataType::Struct(bbox_fields()), true),
        Field::new(
            "links",
            DataType::List(Arc::new(Field::new(
                "item",
                DataType::Struct(link_fields()),
                true,
            ))),
            true,
        ),
        Field::new(
            "assets",
            DataType::Struct(Fields::from(vec![Field::new(
                "data",
                DataType::Struct(data_asset_fields()),
                true,
            )])),
            true,
        ),
        Field::new("collection", DataType::Utf8, true),
        Field::new(
            "datetime",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            true,
        ),
    ]);
    RecordBatch::try_new(
        Arc::new(schema),
        vec![
            string_lists(n, &[]),
            Arc::new(ids),
            Arc::new(geometry),
            Arc::new(bbox),
            Arc::new(links),
            Arc::new(assets),
            Arc::new(collection),
            Arc::new(datetime),
        ],
    )
    .unwrap()
}

/// Returns valid `geo` metadata for the given GeoParquet version.
pub fn geo_metadata(version: &str) -> Value {
    json!({
        "version": version,
        "primary_column": "geometry",
        "columns": {
            "geometry": {
                "encoding": "WKB",
                "geometry_types": ["Point"],
                "bbox": [-180.0, 0.0, 180.0, 9.0],
                "covering": {
                    "bbox": {
                        "xmin": ["bbox", "xmin"],
                        "ymin": ["bbox", "ymin"],
                        "xmax": ["bbox", "xmax"],
                        "ymax": ["bbox", "ymax"]
                    }
                }
            }
        }
    })
}

/// Returns valid `stac-geoparquet` metadata with a single collection.
pub fn stac_metadata() -> Value {
    json!({
        "version": "1.1.0",
        "collections": {
            "test": {
                "type": "Collection",
                "stac_version": "1.1.0",
                "id": "test",
                "description": "A test collection",
                "license": "proprietary",
                "extent": {
                    "spatial": {"bbox": [[-180.0, -90.0, 180.0, 90.0]]},
                    "temporal": {"interval": [["2023-01-01T00:00:00Z", null]]}
                },
                "links": []
            }
        }
    })
}

/// A recipe for a Parquet file, starting from a valid stac-geoparquet file.
pub struct Fixture {
    pub batch: RecordBatch,
    pub kv: Vec<(String, String)>,
    pub props: WriterProperties,
    pub native: bool,
}

impl Fixture {
    /// A valid stac-geoparquet file with zstd compression and large row groups.
    pub fn valid() -> Fixture {
        Fixture {
            batch: stac_batch(10),
            kv: vec![
                ("geo".to_string(), geo_metadata("1.1.0").to_string()),
                ("stac-geoparquet".to_string(), stac_metadata().to_string()),
            ],
            props: WriterProperties::builder()
                .set_compression(Compression::ZSTD(ZstdLevel::default()))
                .set_max_row_group_row_count(Some(100_000))
                .build(),
            native: false,
        }
    }

    /// Writes the fixture to Parquet bytes.
    pub fn bytes(&self) -> Bytes {
        let mut batch = self.batch.clone();
        if self.native {
            let schema = batch.schema();
            let fields: Vec<Field> = schema
                .fields()
                .iter()
                .map(|field| {
                    if field.name() == "geometry" {
                        let mut metadata = field.metadata().clone();
                        metadata.insert(
                            "ARROW:extension:name".to_string(),
                            "geoarrow.wkb".to_string(),
                        );
                        metadata.insert("ARROW:extension:metadata".to_string(), "{}".to_string());
                        field.as_ref().clone().with_metadata(metadata)
                    } else {
                        field.as_ref().clone()
                    }
                })
                .collect();
            batch = RecordBatch::try_new(Arc::new(Schema::new(fields)), batch.columns().to_vec())
                .unwrap();
        }
        let mut props = self.props.clone();
        let kv: Vec<KeyValue> = self
            .kv
            .iter()
            .map(|(key, value)| KeyValue::new(key.clone(), value.clone()))
            .collect();
        props = props
            .into_builder()
            .set_key_value_metadata(Some(kv))
            .build();
        let mut buffer = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut buffer, batch.schema(), Some(props)).unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();
        Bytes::from(buffer)
    }
}
