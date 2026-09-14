; OnObjectDestroyed is a FO76 event Fallout 4 never raises. TrapMain already handles
; FO4's OnDestructionStageChanged, so the timer cancel is re-homed there (calling the
; parent so TrapMain's own disarm path still runs).
; @drop-member OnObjectDestroyed

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
	parent.OnDestructionStageChanged(aiOldStage, aiCurrentStage)
	If Self.IsDestroyed()
		Self.CancelTimer(firingTimerID)
	EndIf
EndEvent

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
        Ammo trapGunAmmo = myWeapons[weaponIndex].weaponToFire.GetAmmo()
        If trapGunAmmo != None
            myWeapons[weaponIndex].weaponToFire.Fire(Self, trapGunAmmo)
        EndIf
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
