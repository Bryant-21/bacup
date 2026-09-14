Event OnEquipped(Actor akActor)
    If akActor != Game.GetPlayer()
        Return
    EndIf

    If myWielder != None
        UnregisterForAnimationEvent(myWielder, "weaponFire")
    EndIf

    myWielder = akActor
    If !RegisterForAnimationEvent(myWielder, "weaponFire")
        myWielder = None
    EndIf
EndEvent

Event OnUnequipped(Actor akActor)
    If myWielder == akActor
        UnregisterForAnimationEvent(myWielder, "weaponFire")
        myWielder = None
    EndIf
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
    If myWielder == None || akSource != myWielder || asEventName != "weaponFire"
        Return
    EndIf

    Ammo expectedAmmo = myAmmo as Ammo
    If expectedAmmo == None
        Return
    EndIf

    Weapon equippedToy = myWielder.GetEquippedWeapon()
    If equippedToy == None || equippedToy.GetAmmo() != expectedAmmo || myWielder.GetItemCount(expectedAmmo) > 1
        Return
    EndIf

    Actor wielder = myWielder
    UnregisterForAnimationEvent(wielder, "weaponFire")
    myWielder = None

    If BreakSFX != None
        BreakSFX.Play(wielder)
    EndIf
    If BreakMSG != None
        BreakMSG.Show()
    EndIf
    wielder.UnequipItem(equippedToy, False, True)
    wielder.RemoveItem(equippedToy, 1, True)
EndEvent
