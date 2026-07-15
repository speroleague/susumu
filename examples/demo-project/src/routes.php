<?php

function validate_php_cart()
{
    load_php_cart();
}

function reserve_php_inventory()
{
    hold_php_stock();
}

function charge_php_customer()
{
    capture_php_payment();
}

function php_checkout()
{
    validate_php_cart();
    reserve_php_inventory();
    charge_php_customer();
}

Route::post('/php-checkout', 'php_checkout');
