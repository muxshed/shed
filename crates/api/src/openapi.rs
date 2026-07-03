// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! OpenAPI document for the public API. Served as JSON at /api/v1/openapi.json
//! with an interactive Scalar docs page at /api/v1/docs. The privileged /admin
//! group is intentionally excluded from this document.

use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "api_key",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-API-Key"))),
        );
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Muxshed API",
        description = "REST API for the Muxshed self-hosted live production studio. Authenticate every request with the X-API-Key header."
    ),
    modifiers(&SecurityAddon),
    security(("api_key" = [])),
    paths(
        crate::routes::status::get_status,
        crate::routes::sources::list,
        crate::routes::sources::create,
        crate::routes::sources::create_from_asset,
        crate::routes::sources::get_one,
        crate::routes::sources::update,
        crate::routes::sources::delete,
        crate::routes::scenes::list,
        crate::routes::scenes::create,
        crate::routes::scenes::get_one,
        crate::routes::scenes::update,
        crate::routes::scenes::delete,
        crate::routes::scenes::activate,
        crate::routes::scenes::add_layer,
        crate::routes::scenes::update_layer,
        crate::routes::scenes::delete_layer,
        crate::routes::destinations::list,
        crate::routes::destinations::create,
        crate::routes::destinations::update,
        crate::routes::destinations::delete,
        crate::routes::destinations::enable,
        crate::routes::destinations::disable,
        crate::routes::stream::start,
        crate::routes::stream::stop,
        crate::routes::recording::start,
        crate::routes::recording::stop,
        crate::routes::recording::status,
        crate::routes::schedules::list,
        crate::routes::schedules::create,
        crate::routes::schedules::update,
        crate::routes::schedules::delete,
        crate::routes::schedules::run_now,
        crate::routes::schedules::stop,
        crate::routes::schedules::get_timezone,
        crate::routes::schedules::set_timezone,
        crate::routes::keys::list,
        crate::routes::keys::create,
        crate::routes::keys::delete,
        crate::routes::channel::get_channel,
        crate::routes::channel::update_channel,
        crate::routes::channel::regenerate_token,
        crate::routes::failover::get_config,
        crate::routes::failover::set_config,
        crate::routes::output::get_config,
        crate::routes::output::set_config,
        crate::routes::output::get_stats,
    ),
    components(schemas(
        muxshed_common::Source,
        muxshed_common::SourceKind,
        muxshed_common::SourceState,
        muxshed_common::Scene,
        muxshed_common::Layer,
        muxshed_common::LayerFit,
        muxshed_common::Position,
        muxshed_common::Size,
        muxshed_common::Destination,
        muxshed_common::DestinationKind,
        muxshed_common::PipelineState,
        muxshed_common::ApiKey,
        muxshed_common::ApiScope,
        muxshed_common::RecordingState,
        muxshed_common::FailoverConfig,
        muxshed_common::StingerConfig,
        muxshed_common::StingerAudio,
        muxshed_common::Schedule,
        muxshed_common::ScheduleItem,
        muxshed_common::ScheduleItemKind,
        muxshed_common::ScheduleRun,
        muxshed_common::TriggerKind,
        muxshed_common::EndBehavior,
        muxshed_common::ChannelConfig,
        muxshed_common::ChannelInfo,
        crate::routes::status::StatusResponse,
        crate::routes::sources::CreateSource,
        crate::routes::sources::UpdateSource,
        crate::routes::sources::CreateFromAsset,
        crate::routes::scenes::CreateScene,
        crate::routes::scenes::UpdateScene,
        crate::routes::scenes::CreateLayer,
        crate::routes::scenes::UpdateLayer,
        crate::routes::destinations::CreateDestination,
        crate::routes::destinations::UpdateDestination,
        crate::routes::recording::RecordingResponse,
        crate::routes::schedules::CreateItem,
        crate::routes::schedules::UpsertSchedule,
        crate::routes::schedules::TzBody,
        crate::routes::keys::CreateKey,
        crate::routes::keys::KeyResponse,
        crate::routes::keys::CreateKeyResponse,
        crate::routes::channel::UpdateChannel,
        crate::routes::output::OutputConfig,
        crate::routes::output::OutputStats,
    )),
    tags(
        (name = "status", description = "Pipeline status"),
        (name = "sources", description = "Ingest sources"),
        (name = "scenes", description = "Scenes and layers"),
        (name = "destinations", description = "Fan-out destinations"),
        (name = "stream", description = "Stream control"),
        (name = "schedules", description = "Scheduled broadcasts"),
        (name = "channel", description = "Public watch page"),
        (name = "keys", description = "API keys"),
        (name = "recording", description = "Local recording"),
        (name = "overlays", description = "Overlays"),
        (name = "failover", description = "Program failover")
    )
)]
pub struct ApiDoc;
