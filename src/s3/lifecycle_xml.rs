use aws_sdk_s3::types::{
    AbortIncompleteMultipartUpload, BucketLifecycleConfiguration, ExpirationStatus,
    LifecycleExpiration, LifecycleRule, LifecycleRuleAndOperator, LifecycleRuleFilter,
    NoncurrentVersionExpiration, NoncurrentVersionTransition, Tag, Transition as S3Transition,
    TransitionDefaultMinimumObjectSize, TransitionStorageClass,
};
use aws_smithy_types::{DateTime, date_time::Format};
use quick_xml::{de::from_str as from_xml_str, se::to_string as to_xml_string};
use serde::{Deserialize, Serialize};

const S3_LIFECYCLE_XMLNS: &str = "http://s3.amazonaws.com/doc/2006-03-01/";

pub fn normalize_xml(xml: &str) -> Result<String, String> {
    let parsed: LifecycleConfigurationXml = from_xml_str(xml).map_err(|error| error.to_string())?;
    parsed.to_xml_string()
}

pub fn lifecycle_configuration_from_xml(
    xml: &str,
) -> Result<
    (
        BucketLifecycleConfiguration,
        Option<TransitionDefaultMinimumObjectSize>,
    ),
    String,
> {
    let parsed: LifecycleConfigurationXml = from_xml_str(xml).map_err(|error| error.to_string())?;
    parsed.into_sdk()
}

pub fn lifecycle_configuration_to_xml(
    config: &BucketLifecycleConfiguration,
    transition_default_minimum_object_size: Option<&TransitionDefaultMinimumObjectSize>,
) -> Result<String, String> {
    LifecycleConfigurationXml::from_sdk(config, transition_default_minimum_object_size)?
        .to_xml_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename = "LifecycleConfiguration")]
struct LifecycleConfigurationXml {
    #[serde(rename = "@xmlns", default, skip_serializing_if = "Option::is_none")]
    xmlns: Option<String>,
    #[serde(rename = "Rule", default)]
    rules: Vec<LifecycleRuleXml>,
    #[serde(
        rename = "TransitionDefaultMinimumObjectSize",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    transition_default_minimum_object_size: Option<String>,
}

impl LifecycleConfigurationXml {
    fn to_xml_string(&self) -> Result<String, String> {
        to_xml_string(self).map_err(|error| error.to_string())
    }

    fn into_sdk(
        self,
    ) -> Result<
        (
            BucketLifecycleConfiguration,
            Option<TransitionDefaultMinimumObjectSize>,
        ),
        String,
    > {
        let rules = self
            .rules
            .into_iter()
            .map(LifecycleRuleXml::into_sdk)
            .collect::<Result<Vec<_>, _>>()?;

        let transition_default_minimum_object_size = self
            .transition_default_minimum_object_size
            .as_deref()
            .map(parse_transition_default_minimum_object_size)
            .transpose()?;

        let config = BucketLifecycleConfiguration::builder()
            .set_rules(Some(rules))
            .build()
            .map_err(|error| error.to_string())?;

        Ok((config, transition_default_minimum_object_size))
    }

    fn from_sdk(
        config: &BucketLifecycleConfiguration,
        transition_default_minimum_object_size: Option<&TransitionDefaultMinimumObjectSize>,
    ) -> Result<Self, String> {
        Ok(Self {
            xmlns: Some(S3_LIFECYCLE_XMLNS.to_string()),
            rules: config
                .rules()
                .iter()
                .map(LifecycleRuleXml::from_sdk)
                .collect::<Result<Vec<_>, _>>()?,
            transition_default_minimum_object_size: transition_default_minimum_object_size
                .map(|value| value.as_str().to_string()),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LifecycleRuleXml {
    #[serde(
        rename = "Expiration",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    expiration: Option<LifecycleExpirationXml>,
    #[serde(rename = "ID", default, skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(rename = "Prefix", default, skip_serializing_if = "Option::is_none")]
    prefix: Option<String>,
    #[serde(rename = "Filter", default, skip_serializing_if = "Option::is_none")]
    filter: Option<LifecycleRuleFilterXml>,
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "Transition", default, skip_serializing_if = "Vec::is_empty")]
    transitions: Vec<TransitionXml>,
    #[serde(
        rename = "NoncurrentVersionTransition",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    noncurrent_version_transitions: Vec<NoncurrentVersionTransitionXml>,
    #[serde(
        rename = "NoncurrentVersionExpiration",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    noncurrent_version_expiration: Option<NoncurrentVersionExpirationXml>,
    #[serde(
        rename = "AbortIncompleteMultipartUpload",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    abort_incomplete_multipart_upload: Option<AbortIncompleteMultipartUploadXml>,
}

impl LifecycleRuleXml {
    #[allow(deprecated)]
    fn into_sdk(self) -> Result<LifecycleRule, String> {
        let status = parse_expiration_status(&self.status)?;
        LifecycleRule::builder()
            .set_expiration(
                self.expiration
                    .map(LifecycleExpirationXml::into_sdk)
                    .transpose()?,
            )
            .set_id(self.id)
            .set_prefix(self.prefix)
            .set_filter(
                self.filter
                    .map(LifecycleRuleFilterXml::into_sdk)
                    .transpose()?,
            )
            .status(status)
            .set_transitions(
                (!self.transitions.is_empty())
                    .then(|| {
                        self.transitions
                            .into_iter()
                            .map(TransitionXml::into_sdk)
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?,
            )
            .set_noncurrent_version_transitions(
                (!self.noncurrent_version_transitions.is_empty())
                    .then(|| {
                        self.noncurrent_version_transitions
                            .into_iter()
                            .map(NoncurrentVersionTransitionXml::into_sdk)
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?,
            )
            .set_noncurrent_version_expiration(
                self.noncurrent_version_expiration
                    .map(NoncurrentVersionExpirationXml::into_sdk)
                    .transpose()?,
            )
            .set_abort_incomplete_multipart_upload(
                self.abort_incomplete_multipart_upload
                    .map(AbortIncompleteMultipartUploadXml::into_sdk)
                    .transpose()?,
            )
            .build()
            .map_err(|error| error.to_string())
    }

    #[allow(deprecated)]
    fn from_sdk(rule: &LifecycleRule) -> Result<Self, String> {
        Ok(Self {
            expiration: rule
                .expiration()
                .map(LifecycleExpirationXml::from_sdk)
                .transpose()?,
            id: rule.id().map(ToOwned::to_owned),
            prefix: rule.prefix().map(ToOwned::to_owned),
            filter: rule
                .filter()
                .map(LifecycleRuleFilterXml::from_sdk)
                .transpose()?,
            status: rule.status().as_str().to_string(),
            transitions: rule
                .transitions()
                .iter()
                .map(TransitionXml::from_sdk)
                .collect::<Result<Vec<_>, _>>()?,
            noncurrent_version_transitions: rule
                .noncurrent_version_transitions()
                .iter()
                .map(NoncurrentVersionTransitionXml::from_sdk)
                .collect::<Result<Vec<_>, _>>()?,
            noncurrent_version_expiration: rule
                .noncurrent_version_expiration()
                .map(NoncurrentVersionExpirationXml::from_sdk)
                .transpose()?,
            abort_incomplete_multipart_upload: rule
                .abort_incomplete_multipart_upload()
                .map(AbortIncompleteMultipartUploadXml::from_sdk)
                .transpose()?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LifecycleRuleFilterXml {
    #[serde(rename = "Prefix", default, skip_serializing_if = "Option::is_none")]
    prefix: Option<String>,
    #[serde(rename = "Tag", default, skip_serializing_if = "Option::is_none")]
    tag: Option<TagXml>,
    #[serde(
        rename = "ObjectSizeGreaterThan",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    object_size_greater_than: Option<i64>,
    #[serde(
        rename = "ObjectSizeLessThan",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    object_size_less_than: Option<i64>,
    #[serde(rename = "And", default, skip_serializing_if = "Option::is_none")]
    and: Option<LifecycleRuleAndOperatorXml>,
}

impl LifecycleRuleFilterXml {
    fn into_sdk(self) -> Result<LifecycleRuleFilter, String> {
        Ok(LifecycleRuleFilter::builder()
            .set_prefix(self.prefix)
            .set_tag(self.tag.map(TagXml::into_sdk).transpose()?)
            .set_object_size_greater_than(self.object_size_greater_than)
            .set_object_size_less_than(self.object_size_less_than)
            .set_and(
                self.and
                    .map(LifecycleRuleAndOperatorXml::into_sdk)
                    .transpose()?,
            )
            .build())
    }

    fn from_sdk(filter: &LifecycleRuleFilter) -> Result<Self, String> {
        Ok(Self {
            prefix: filter.prefix().map(ToOwned::to_owned),
            tag: filter.tag().map(TagXml::from_sdk).transpose()?,
            object_size_greater_than: filter.object_size_greater_than(),
            object_size_less_than: filter.object_size_less_than(),
            and: filter
                .and()
                .map(LifecycleRuleAndOperatorXml::from_sdk)
                .transpose()?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LifecycleRuleAndOperatorXml {
    #[serde(rename = "Prefix", default, skip_serializing_if = "Option::is_none")]
    prefix: Option<String>,
    #[serde(rename = "Tag", default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<TagXml>,
    #[serde(
        rename = "ObjectSizeGreaterThan",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    object_size_greater_than: Option<i64>,
    #[serde(
        rename = "ObjectSizeLessThan",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    object_size_less_than: Option<i64>,
}

impl LifecycleRuleAndOperatorXml {
    fn into_sdk(self) -> Result<LifecycleRuleAndOperator, String> {
        Ok(LifecycleRuleAndOperator::builder()
            .set_prefix(self.prefix)
            .set_tags(
                (!self.tags.is_empty())
                    .then(|| {
                        self.tags
                            .into_iter()
                            .map(TagXml::into_sdk)
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?,
            )
            .set_object_size_greater_than(self.object_size_greater_than)
            .set_object_size_less_than(self.object_size_less_than)
            .build())
    }

    fn from_sdk(and: &LifecycleRuleAndOperator) -> Result<Self, String> {
        Ok(Self {
            prefix: and.prefix().map(ToOwned::to_owned),
            tags: and
                .tags()
                .iter()
                .map(TagXml::from_sdk)
                .collect::<Result<Vec<_>, _>>()?,
            object_size_greater_than: and.object_size_greater_than(),
            object_size_less_than: and.object_size_less_than(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LifecycleExpirationXml {
    #[serde(rename = "Date", default, skip_serializing_if = "Option::is_none")]
    date: Option<String>,
    #[serde(rename = "Days", default, skip_serializing_if = "Option::is_none")]
    days: Option<i32>,
    #[serde(
        rename = "ExpiredObjectDeleteMarker",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    expired_object_delete_marker: Option<bool>,
}

impl LifecycleExpirationXml {
    fn into_sdk(self) -> Result<LifecycleExpiration, String> {
        Ok(LifecycleExpiration::builder()
            .set_date(self.date.as_deref().map(parse_datetime).transpose()?)
            .set_days(self.days)
            .set_expired_object_delete_marker(self.expired_object_delete_marker)
            .build())
    }

    fn from_sdk(expiration: &LifecycleExpiration) -> Result<Self, String> {
        Ok(Self {
            date: expiration.date().map(format_datetime).transpose()?,
            days: expiration.days(),
            expired_object_delete_marker: expiration.expired_object_delete_marker(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TransitionXml {
    #[serde(rename = "Date", default, skip_serializing_if = "Option::is_none")]
    date: Option<String>,
    #[serde(rename = "Days", default, skip_serializing_if = "Option::is_none")]
    days: Option<i32>,
    #[serde(
        rename = "StorageClass",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    storage_class: Option<String>,
}

impl TransitionXml {
    fn into_sdk(self) -> Result<S3Transition, String> {
        Ok(S3Transition::builder()
            .set_date(self.date.as_deref().map(parse_datetime).transpose()?)
            .set_days(self.days)
            .set_storage_class(
                self.storage_class
                    .as_deref()
                    .map(parse_transition_storage_class)
                    .transpose()?,
            )
            .build())
    }

    fn from_sdk(transition: &S3Transition) -> Result<Self, String> {
        Ok(Self {
            date: transition.date().map(format_datetime).transpose()?,
            days: transition.days(),
            storage_class: transition
                .storage_class()
                .map(|value| value.as_str().to_string()),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct NoncurrentVersionTransitionXml {
    #[serde(
        rename = "NoncurrentDays",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    noncurrent_days: Option<i32>,
    #[serde(
        rename = "StorageClass",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    storage_class: Option<String>,
    #[serde(
        rename = "NewerNoncurrentVersions",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    newer_noncurrent_versions: Option<i32>,
}

impl NoncurrentVersionTransitionXml {
    fn into_sdk(self) -> Result<NoncurrentVersionTransition, String> {
        Ok(NoncurrentVersionTransition::builder()
            .set_noncurrent_days(self.noncurrent_days)
            .set_storage_class(
                self.storage_class
                    .as_deref()
                    .map(parse_transition_storage_class)
                    .transpose()?,
            )
            .set_newer_noncurrent_versions(self.newer_noncurrent_versions)
            .build())
    }

    fn from_sdk(transition: &NoncurrentVersionTransition) -> Result<Self, String> {
        Ok(Self {
            noncurrent_days: transition.noncurrent_days(),
            storage_class: transition
                .storage_class()
                .map(|value| value.as_str().to_string()),
            newer_noncurrent_versions: transition.newer_noncurrent_versions(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct NoncurrentVersionExpirationXml {
    #[serde(
        rename = "NoncurrentDays",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    noncurrent_days: Option<i32>,
    #[serde(
        rename = "NewerNoncurrentVersions",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    newer_noncurrent_versions: Option<i32>,
}

impl NoncurrentVersionExpirationXml {
    fn into_sdk(self) -> Result<NoncurrentVersionExpiration, String> {
        Ok(NoncurrentVersionExpiration::builder()
            .set_noncurrent_days(self.noncurrent_days)
            .set_newer_noncurrent_versions(self.newer_noncurrent_versions)
            .build())
    }

    fn from_sdk(expiration: &NoncurrentVersionExpiration) -> Result<Self, String> {
        Ok(Self {
            noncurrent_days: expiration.noncurrent_days(),
            newer_noncurrent_versions: expiration.newer_noncurrent_versions(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct AbortIncompleteMultipartUploadXml {
    #[serde(
        rename = "DaysAfterInitiation",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    days_after_initiation: Option<i32>,
}

impl AbortIncompleteMultipartUploadXml {
    fn into_sdk(self) -> Result<AbortIncompleteMultipartUpload, String> {
        Ok(AbortIncompleteMultipartUpload::builder()
            .set_days_after_initiation(self.days_after_initiation)
            .build())
    }

    fn from_sdk(upload: &AbortIncompleteMultipartUpload) -> Result<Self, String> {
        Ok(Self {
            days_after_initiation: upload.days_after_initiation(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TagXml {
    #[serde(rename = "Key")]
    key: String,
    #[serde(rename = "Value")]
    value: String,
}

impl TagXml {
    fn into_sdk(self) -> Result<Tag, String> {
        Tag::builder()
            .key(self.key)
            .value(self.value)
            .build()
            .map_err(|error| error.to_string())
    }

    fn from_sdk(tag: &Tag) -> Result<Self, String> {
        Ok(Self {
            key: tag.key().to_string(),
            value: tag.value().to_string(),
        })
    }
}

fn parse_datetime(value: &str) -> Result<DateTime, String> {
    DateTime::from_str(value, Format::DateTime).map_err(|error| error.to_string())
}

fn format_datetime(value: &DateTime) -> Result<String, String> {
    value
        .fmt(Format::DateTime)
        .map_err(|error| error.to_string())
}

fn parse_expiration_status(value: &str) -> Result<ExpirationStatus, String> {
    ExpirationStatus::try_parse(value).map_err(|error| error.to_string())
}

fn parse_transition_storage_class(value: &str) -> Result<TransitionStorageClass, String> {
    TransitionStorageClass::try_parse(value).map_err(|error| error.to_string())
}

fn parse_transition_default_minimum_object_size(
    value: &str,
) -> Result<TransitionDefaultMinimumObjectSize, String> {
    TransitionDefaultMinimumObjectSize::try_parse(value).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_xml_round_trips_full_configuration() {
        let xml = r#"<LifecycleConfiguration>
  <Rule>
    <ID>archive</ID>
    <Filter>
      <And>
        <Prefix>logs/</Prefix>
        <Tag>
          <Key>env</Key>
          <Value>prod</Value>
        </Tag>
        <ObjectSizeGreaterThan>100</ObjectSizeGreaterThan>
      </And>
    </Filter>
    <Status>Enabled</Status>
    <Expiration>
      <Days>30</Days>
      <ExpiredObjectDeleteMarker>false</ExpiredObjectDeleteMarker>
    </Expiration>
    <Transition>
      <Days>7</Days>
      <StorageClass>STANDARD_IA</StorageClass>
    </Transition>
    <NoncurrentVersionTransition>
      <NoncurrentDays>14</NoncurrentDays>
      <StorageClass>GLACIER</StorageClass>
      <NewerNoncurrentVersions>2</NewerNoncurrentVersions>
    </NoncurrentVersionTransition>
    <NoncurrentVersionExpiration>
      <NoncurrentDays>60</NoncurrentDays>
      <NewerNoncurrentVersions>3</NewerNoncurrentVersions>
    </NoncurrentVersionExpiration>
    <AbortIncompleteMultipartUpload>
      <DaysAfterInitiation>5</DaysAfterInitiation>
    </AbortIncompleteMultipartUpload>
  </Rule>
  <TransitionDefaultMinimumObjectSize>all_storage_classes_128K</TransitionDefaultMinimumObjectSize>
</LifecycleConfiguration>"#;

        let normalized = normalize_xml(xml).expect("valid lifecycle xml");
        let (config, default_minimum_size) =
            lifecycle_configuration_from_xml(&normalized).expect("parse normalized xml");
        let xml_round_trip = lifecycle_configuration_to_xml(&config, default_minimum_size.as_ref())
            .expect("serialize lifecycle config");

        assert!(xml_round_trip.contains("LifecycleConfiguration"));
        assert!(xml_round_trip.contains("TransitionDefaultMinimumObjectSize"));
        assert_eq!(config.rules().len(), 1);
        assert_eq!(
            default_minimum_size.expect("default minimum size").as_str(),
            "all_storage_classes_128K"
        );
    }

    #[test]
    fn invalid_xml_fails() {
        let error = normalize_xml("<LifecycleConfiguration>").expect_err("invalid xml");
        assert!(!error.is_empty());
    }
}
