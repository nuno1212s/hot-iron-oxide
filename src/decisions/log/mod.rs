use atlas_common::ordering::Orderable;
use atlas_common::error::*;
use atlas_core::smr::smr_decision_log::ShareableMessage;
use crate::messages::{HotIronOxMsg, QC};
use crate::view::View;

pub enum PrepareLog<D> {
    Leader {
        received_prepare_certificate: Vec<QC<D>>,
        highest_prepare_certificate: Option<QC<D>>,
    },
    Replica {},
}


/// Decision log
pub struct DecisionLog<D> {
    prepare_log: PrepareLog<D>,

}

impl<D> DecisionLog<D> {
    pub fn handle_new_view_prepareQC_received(&mut self, view: &View, message: QC<D>) {
        match &mut self.prepare_log {
            PrepareLog::Leader { received_prepare_certificate, .. } => {
                received_prepare_certificate.push(message);
            }
            PrepareLog::Replica { .. } => {}
        }
    }

    pub fn populate_highest_prepareQC(&mut self, view: &View) -> Result<Option<&QC<D>>> {
        match &mut self.prepare_log {
            PrepareLog::Leader { received_prepare_certificate, highest_prepare_certificate } => {
                if received_prepare_certificate.is_empty() {
                    return Err(Error::simple_with_msg(ErrorKind::Consensus, "Received prepare certificate is empty"));
                } else {
                    received_prepare_certificate.sort_by(|c1, c2| c1.sequence_number().cmp(&c2.sequence_number()));

                    Ok(Some(received_prepare_certificate.last().unwrap()))
                }
            }
            PrepareLog::Replica { .. } => {
                Ok(None)
            }
        }
    }
}