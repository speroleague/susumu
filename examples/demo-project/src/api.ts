import express from "express";

const router = express.Router();

function validateCart() {
  loadCart();
  checkPrices();
}

function reserveInventory() {
  loadCatalog();
  holdStock();
}

function capturePayment() {
  chargeGateway();
  writePaymentLedger();
}

function notifyCustomer() {
  sendReceiptEmail();
}

function auditOrder() {
  writeAuditEvent();
}

function checkout() {
  validateCart();
  reserveInventory();
  capturePayment();
  notifyCustomer();
  auditOrder();
}

function listOrders() {
  loadOrders();
}

router.post("/checkout", checkout);
router.get("/orders", listOrders);
