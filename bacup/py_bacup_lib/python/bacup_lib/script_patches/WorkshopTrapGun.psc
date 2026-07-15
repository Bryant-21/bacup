; Fire each bound weapon in sequence. Weapon.Fire is the FO4-native projectile
; path; no ammo or projectile is invented when the VMAD does not bind one.

Function ClientFireTrap()
    firingCount = 0
    FireNextWeapon()
EndFunction

Function FireNextWeapon()
    If myWeapons == None || myWeapons.Length == 0
        Return
    EndIf

    Int weaponIndex = firingCount % myWeapons.Length
    If myWeapons[weaponIndex].weaponToFire != None
        myWeapons[weaponIndex].weaponToFire.Fire(Self)
    EndIf
    If myWeapons[weaponIndex].weaponSound != None
        myWeapons[weaponIndex].weaponSound.Play(Self)
    EndIf

    firingCount = firingCount + 1
    Int shotsToFire = firingCountMax
    If shotsToFire <= 0
        shotsToFire = myWeapons.Length
    EndIf
    If firingCount < shotsToFire && firingTime > 0.0
        StartTimer(firingTime, firingTimerID)
    Else
        GoToState("fired")
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == firingTimerID
        FireNextWeapon()
    EndIf
EndEvent
