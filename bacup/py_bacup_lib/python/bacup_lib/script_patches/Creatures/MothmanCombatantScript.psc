Function RegisterCombatantEvents()
	If selfRef == None
		Return
	EndIf

	RegisterForRemoteEvent(selfRef, "OnCombatStateChanged")
	RegisterForAnimationEvent(selfRef, animEventAoEAttackStart)
	RegisterForAnimationEvent(selfRef, animEventTeleportStart)
EndFunction

Function UnregisterCombatantEvents()
	If selfRef == None
		Return
	EndIf

	UnregisterForRemoteEvent(selfRef, "OnCombatStateChanged")
	UnregisterForAnimationEvent(selfRef, animEventAoEAttackStart)
	UnregisterForAnimationEvent(selfRef, animEventTeleportStart)
EndFunction

Function StartAoEWeaponTimer(Float afDelayMin, Float afDelayMax)
	If selfRef == None || selfRef.IsDead() || AoEAttackWeapon == None
		Return
	EndIf

	Float delayMax = afDelayMax
	If delayMax < afDelayMin
		delayMax = afDelayMin
	EndIf
	CancelTimer(weaponAoEEquipTimerID)
	StartTimer(Utility.RandomFloat(afDelayMin, delayMax), weaponAoEEquipTimerID)
EndFunction

Event OnEffectStart(Actor akTarget, Actor akCaster)
	selfRef = akCaster
	If selfRef == None
		selfRef = akTarget
	EndIf
	If selfRef == None || selfRef.IsDead()
		Return
	EndIf

	RegisterCombatantEvents()
	If selfRef.GetCombatState() == CombatState_InCombat
		StartAoEWeaponTimer(InitialAoEAttackTimeMin, InitialAoEAttackTimeMax)
	EndIf
EndEvent

Event Actor.OnCombatStateChanged(Actor akSender, Actor akTarget, Int aeCombatState)
	If akSender != selfRef || selfRef == None || selfRef.IsDead()
		Return
	EndIf

	If aeCombatState == CombatState_InCombat
		StartAoEWeaponTimer(InitialAoEAttackTimeMin, InitialAoEAttackTimeMax)
	Else
		CancelTimer(weaponAoEEquipTimerID)
		If AoEAttackWeapon != None
			selfRef.UnequipItem(AoEAttackWeapon, False, True)
		EndIf
	EndIf
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
	If selfRef == None || selfRef.IsDead() || akSource != selfRef
		Return
	EndIf

	If asEventName == animEventAoEAttackStart
		If AoEAttackWeapon != None
			selfRef.UnequipItem(AoEAttackWeapon, False, True)
		EndIf
		StartAoEWeaponTimer(AoEAttackDelayTime, AoEAttackDelayTime)
	ElseIf asEventName == animEventTeleportStart
		If GetState() == "Disappear"
			If DisappearExplosion != None
				selfRef.PlaceAtMe(DisappearExplosion)
			EndIf
		ElseIf EnterTeleportExplosion != None
			selfRef.PlaceAtMe(EnterTeleportExplosion)
		EndIf
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == weaponAoEEquipTimerID && selfRef != None && !selfRef.IsDead() && selfRef.GetCombatState() == CombatState_InCombat && AoEAttackWeapon != None
		selfRef.EquipItem(AoEAttackWeapon, False, True)
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	CancelTimer(weaponAoEEquipTimerID)
	CancelTimerGameTime(sunriseTimerID)
	UnregisterCombatantEvents()
	If selfRef != None && !selfRef.IsDead() && AoEAttackWeapon != None
		selfRef.EquipItem(AoEAttackWeapon, False, True)
	EndIf
	selfRef = None
EndEvent
