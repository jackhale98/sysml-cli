package AnalysisTradeTest {
    private import ScalarValues::*;

    part def Engine {
        attribute mass : Real;
        attribute power : Real;
        attribute cost : Real;
    }

    part def Vehicle {
        attribute mass : Real = 1000;
        part engine : Engine;
    }

    analysis def FuelAnalysis {
        subject vehicle : Vehicle;
        in attribute scenario : Real;
        attribute distance : Real = 100;
        return fuelEconomy : Real;
    }

    analysis def EngineTradeOff {
        subject alternatives : Engine;

        objective : MaximizeObjective;

        part engine4cyl : Engine;
        part engine6cyl : Engine;

        return selectedEngine : Engine;
    }
}
