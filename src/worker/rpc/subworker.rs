use std::path::Path;

use std::sync::Arc;

use common::id::DataObjectId;
use common::convert::FromCapnp;
use worker::{StateRef, State};
use worker::graph::SubworkerRef;
use worker::data::{Data, DataType, Storage};
use worker::fs::workdir::WorkDir;
use subworker_capnp::subworker_upstream;
use futures::Future;
use capnp;
use capnp::capability::Promise;


use errors::Result;

use SUBWORKER_PROTOCOL_VERSION;

pub struct SubworkerUpstreamImpl {
    state: StateRef,
}

impl SubworkerUpstreamImpl {
    pub fn new(state: &StateRef) -> Self {
        Self { state: state.clone() }
    }
}

impl Drop for SubworkerUpstreamImpl {
    fn drop(&mut self) {
        panic!("Lost connection to subworker");
    }
}

impl subworker_upstream::Server for SubworkerUpstreamImpl {
    fn register(
        &mut self,
        params: subworker_upstream::RegisterParams,
        mut _results: subworker_upstream::RegisterResults,
    ) -> Promise<(), ::capnp::Error> {
        let params = pry!(params.get());

        if params.get_version() != SUBWORKER_PROTOCOL_VERSION {
            return Promise::err(capnp::Error::failed(format!(
                "Invalid subworker protocol; expected = {}",
                SUBWORKER_PROTOCOL_VERSION
            )));
        }

        let subworker_type = pry!(params.get_subworker_type());
        let control = pry!(params.get_control());

        pry!(
            self.state
                .get_mut()
                .add_subworker(
                    params.get_subworker_id(),
                    subworker_type.to_string(),
                    control,
                )
                .map_err(|e| ::capnp::Error::failed(e.description().into()))
        );
        Promise::ok(())
    }
}

pub fn data_from_capnp(
    state: &State,
    subworker_dir: &Path,
    reader: &::capnp_gen::subworker_capnp::local_data::Reader,
) -> Result<Arc<Data>> {
    let data_type = reader.get_type()?;
    assert!(data_type == ::capnp_gen::common_capnp::DataObjectType::Blob);
    match reader.get_storage().which()? {
        ::capnp_gen::subworker_capnp::local_data::storage::Memory(data) => Ok(Arc::new(Data::new(
            DataType::Blob,
            Storage::Memory(
                data?.into(),
            ),
        ))),
        ::capnp_gen::subworker_capnp::local_data::storage::Path(data) => {
            let source_path = Path::new(data?);
            if (!source_path.is_absolute()) {
                bail!("Path of dataobject is not absolute");
            }
            if (!source_path.starts_with(subworker_dir)) {
                bail!("Path of dataobject is not in subworker dir");
            }
            let target_path = state.work_dir().new_path_for_dataobject();
            Ok(Arc::new(
                Data::new_by_fs_move(&Path::new(source_path), target_path)?,
            ))
        }
        ::capnp_gen::subworker_capnp::local_data::storage::InWorker(data) => {
            let object_id = DataObjectId::from_capnp(&data?);
            let object = state.object_by_id(object_id)?;
            let data = object.get().data().clone();
            Ok(data)
        }
        _ => unimplemented!(),
    }
}
