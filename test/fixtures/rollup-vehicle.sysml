package RollupTest {
    private import ScalarValues::*;
    part def Engine {
        attribute mass : Real = 180;
        attribute cost : Real = 5000;
        attribute power : Real = 150;
    }

    part def Chassis {
        attribute mass : Real = 250;
        attribute cost : Real = 3000;
    }

    part def Wheel {
        attribute mass : Real = 12.5;
        attribute cost : Real = 200;
    }

    part def Body {
        attribute mass : Real = 400;
        attribute cost : Real = 8000;
    }

    part def Vehicle {
        attribute mass : Real = 20;
        attribute cost : Real = 500;
        part engine : Engine;
        part chassis : Chassis;
        part wheels : Wheel [4];
        part body : Body;
    }
}
