// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Vendor.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Vendor.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Vendor.cs`.

use empyrean_entity::enums::{PropertyBool, PropertyDataId, PropertyFloat, PropertyInt};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Vendor.AlternateCurrency
    pub fn alternate_currency(&self) -> Option<u32> {
        self.get_property(PropertyDataId::AlternateCurrency)
    }

    // ACE: Vendor.AlternateCurrency
    pub fn set_alternate_currency(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::AlternateCurrency),
            Some(v) => self.set_property(PropertyDataId::AlternateCurrency, v),
        }
    }

    // ACE: Vendor.OpenForBusiness
    pub fn open_for_business(&self) -> bool {
        self.get_property(PropertyBool::OpenForBusiness)
            .unwrap_or(true)
    }

    // ACE: Vendor.OpenForBusiness
    pub fn set_open_for_business(&mut self, value: bool) {
        if value {
            self.remove_property(PropertyBool::OpenForBusiness);
        } else {
            self.set_property(PropertyBool::OpenForBusiness, value);
        }
    }

    // ACE: Vendor.MerchandiseItemTypes
    pub fn merchandise_item_types(&self) -> Option<i32> {
        self.get_property(PropertyInt::MerchandiseItemTypes)
    }

    // ACE: Vendor.MerchandiseItemTypes
    pub fn set_merchandise_item_types(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::MerchandiseItemTypes),
            Some(v) => self.set_property(PropertyInt::MerchandiseItemTypes, v),
        }
    }

    // ACE: Vendor.MerchandiseMinValue
    pub fn merchandise_min_value(&self) -> Option<i32> {
        self.get_property(PropertyInt::MerchandiseMinValue)
    }

    // ACE: Vendor.MerchandiseMinValue
    pub fn set_merchandise_min_value(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::MerchandiseMinValue),
            Some(v) => self.set_property(PropertyInt::MerchandiseMinValue, v),
        }
    }

    // ACE: Vendor.MerchandiseMaxValue
    pub fn merchandise_max_value(&self) -> Option<i32> {
        self.get_property(PropertyInt::MerchandiseMaxValue)
    }

    // ACE: Vendor.MerchandiseMaxValue
    pub fn set_merchandise_max_value(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::MerchandiseMaxValue),
            Some(v) => self.set_property(PropertyInt::MerchandiseMaxValue, v),
        }
    }

    // ACE: Vendor.BuyPrice
    pub fn buy_price(&self) -> Option<f64> {
        self.get_property(PropertyFloat::BuyPrice)
    }

    // ACE: Vendor.BuyPrice
    pub fn set_buy_price(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::BuyPrice),
            Some(v) => self.set_property(PropertyFloat::BuyPrice, v),
        }
    }

    // ACE: Vendor.SellPrice
    pub fn sell_price(&self) -> Option<f64> {
        self.get_property(PropertyFloat::SellPrice)
    }

    // ACE: Vendor.SellPrice
    pub fn set_sell_price(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::SellPrice),
            Some(v) => self.set_property(PropertyFloat::SellPrice, v),
        }
    }

    // ACE: Vendor.DealMagicalItems
    pub fn deal_magical_items(&self) -> Option<bool> {
        self.get_property(PropertyBool::DealMagicalItems)
    }

    // ACE: Vendor.DealMagicalItems
    pub fn set_deal_magical_items(&mut self, value: Option<bool>) {
        match value {
            None => self.remove_property(PropertyBool::DealMagicalItems),
            Some(v) => self.set_property(PropertyBool::DealMagicalItems, v),
        }
    }

    // ACE: Vendor.VendorService
    pub fn vendor_service(&self) -> Option<bool> {
        self.get_property(PropertyBool::VendorService)
    }

    // ACE: Vendor.VendorService
    pub fn set_vendor_service(&mut self, value: Option<bool>) {
        match value {
            None => self.remove_property(PropertyBool::VendorService),
            Some(v) => self.set_property(PropertyBool::VendorService, v),
        }
    }

    // ACE: Vendor.VendorHappyMean
    pub fn vendor_happy_mean(&self) -> Option<i32> {
        self.get_property(PropertyInt::VendorHappyMean)
    }

    // ACE: Vendor.VendorHappyMean
    pub fn set_vendor_happy_mean(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::VendorHappyMean),
            Some(v) => self.set_property(PropertyInt::VendorHappyMean, v),
        }
    }

    // ACE: Vendor.VendorHappyVariance
    pub fn vendor_happy_variance(&self) -> Option<i32> {
        self.get_property(PropertyInt::VendorHappyVariance)
    }

    // ACE: Vendor.VendorHappyVariance
    pub fn set_vendor_happy_variance(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::VendorHappyVariance),
            Some(v) => self.set_property(PropertyInt::VendorHappyVariance, v),
        }
    }

    // ACE: Vendor.VendorHappyMaxItems
    pub fn vendor_happy_max_items(&self) -> Option<i32> {
        self.get_property(PropertyInt::VendorHappyMaxItems)
    }

    // ACE: Vendor.VendorHappyMaxItems
    pub fn set_vendor_happy_max_items(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::VendorHappyMaxItems),
            Some(v) => self.set_property(PropertyInt::VendorHappyMaxItems, v),
        }
    }

    // ACE: Vendor.NumItemsSold
    pub fn num_items_sold(&self) -> i32 {
        self.get_property(PropertyInt::NumItemsSold).unwrap_or(0)
    }

    // ACE: Vendor.NumItemsSold
    pub fn set_num_items_sold(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::NumItemsSold);
        } else {
            self.set_property(PropertyInt::NumItemsSold, value);
        }
    }

    // ACE: Vendor.NumItemsBought
    pub fn num_items_bought(&self) -> i32 {
        self.get_property(PropertyInt::NumItemsBought).unwrap_or(0)
    }

    // ACE: Vendor.NumItemsBought
    pub fn set_num_items_bought(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::NumItemsBought);
        } else {
            self.set_property(PropertyInt::NumItemsBought, value);
        }
    }

    // ACE: Vendor.NumServicesSold
    pub fn num_services_sold(&self) -> i32 {
        self.get_property(PropertyInt::NumServicesSold).unwrap_or(0)
    }

    // ACE: Vendor.NumServicesSold
    pub fn set_num_services_sold(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::NumServicesSold);
        } else {
            self.set_property(PropertyInt::NumServicesSold, value);
        }
    }

    // ACE: Vendor.MoneyIncome
    pub fn money_income(&self) -> i32 {
        self.get_property(PropertyInt::MoneyIncome).unwrap_or(0)
    }

    // ACE: Vendor.MoneyIncome
    pub fn set_money_income(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::MoneyIncome);
        } else {
            self.set_property(PropertyInt::MoneyIncome, value);
        }
    }

    // ACE: Vendor.MoneyOutflow
    pub fn money_outflow(&self) -> i32 {
        self.get_property(PropertyInt::MoneyOutflow).unwrap_or(0)
    }

    // ACE: Vendor.MoneyOutflow
    pub fn set_money_outflow(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::MoneyOutflow);
        } else {
            self.set_property(PropertyInt::MoneyOutflow, value);
        }
    }
}
