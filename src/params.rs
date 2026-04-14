use crate::data_types::PlcDataType;

pub trait PlcParams {
    fn as_data(self) -> Vec<u8>;
}

impl<P1: PlcDataType> PlcParams for P1 {
    fn as_data(self) -> Vec<u8> {
        self.as_bytes().to_owned()
    }
}

impl<P1: PlcDataType, P2: PlcDataType> PlcParams for (P1, P2) {
    fn as_data(self) -> Vec<u8> {
        let data_1 = self.0.as_bytes();
        let data_2 = self.1.as_bytes();
        [data_1, data_2].concat()
    }
}

impl<P1: PlcDataType, P2: PlcDataType, P3: PlcDataType> PlcParams for (P1, P2, P3) {
    fn as_data(self) -> Vec<u8> {
        let data_1 = self.0.as_bytes();
        let data_2 = self.1.as_bytes();
        let data_3 = self.2.as_bytes();
        [data_1, data_2, data_3].concat()
    }
}

impl<P1: PlcDataType, P2: PlcDataType, P3: PlcDataType, P4: PlcDataType> PlcParams
    for (P1, P2, P3, P4)
{
    fn as_data(self) -> Vec<u8> {
        let data_1 = self.0.as_bytes();
        let data_2 = self.1.as_bytes();
        let data_3 = self.2.as_bytes();
        let data_4 = self.3.as_bytes();
        [data_1, data_2, data_3, data_4].concat()
    }
}

impl<P1: PlcDataType, P2: PlcDataType, P3: PlcDataType, P4: PlcDataType, P5: PlcDataType> PlcParams
    for (P1, P2, P3, P4, P5)
{
    fn as_data(self) -> Vec<u8> {
        let data_1 = self.0.as_bytes();
        let data_2 = self.1.as_bytes();
        let data_3 = self.2.as_bytes();
        let data_4 = self.3.as_bytes();
        let data_5 = self.4.as_bytes();
        [data_1, data_2, data_3, data_4, data_5].concat()
    }
}
