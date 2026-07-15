; Fire the bound bow with its bound ammunition when TrapMain enters firing.

Function ClientFireTrap()
    If FireTrapAnim != ""
        PlayAnimation(FireTrapAnim)
    EndIf
    If MyBowWeapon != None
        MyBowWeapon.Fire(Self, MyAmmo)
    EndIf
EndFunction
