Bool Function IsNonMeleeHit(Form akSource, Projectile akProjectile)
	If akProjectile != None || (akSource as Explosion) != None
		Return True
	EndIf
	Weapon sourceWeapon = akSource as Weapon
	Return sourceWeapon != None && sourceWeapon.GetAmmo() != None
EndFunction

Function ShowMeleeOnlyMessage()
	If MeleeOnlyMSG == None
		Return
	EndIf
	Float now = Utility.GetCurrentRealTime()
	Float elapsed = now - B21LastMeleeOnlyMessageTime
	; Real time restarts on load, so a negative gap means a new session rather than a recent message.
	If B21LastMeleeOnlyMessageTime > 0.0 && elapsed >= 0.0 && elapsed < 5.0
		Return
	EndIf
	B21LastMeleeOnlyMessageTime = now
	MeleeOnlyMSG.Show()
EndFunction

Function BeginCrystalClearing()
	Int index = 0
	While index < GetCount()
		ObjectReference crystal = GetAt(index)
		If crystal != None
			crystal.ClearDestruction()
		EndIf
		index += 1
	EndWhile
	isStageDone = False
	crystalsDestroyed = 0
	RegisterForHitEvent(Self)
	RefreshCrystalCount()
EndFunction

Function RefreshCrystalCount()
	If isStageDone
		Return
	EndIf
	Int total = GetCount()
	Int destroyed = 0
	Int index = 0
	While index < total
		ObjectReference crystal = GetAt(index)
		If crystal != None && crystal.IsDestroyed()
			destroyed += 1
		EndIf
		index += 1
	EndWhile
	crystalsDestroyed = destroyed

	Quest owningQuest = GetOwningQuest()
	B21:QuestVariables questVariables = owningQuest as B21:QuestVariables
	If questVariables != None
		questVariables.SetVariable("Crystals", total as Float)
		questVariables.SetVariable("CrystalsDestroyed", destroyed as Float)
	EndIf
	If total > 0 && destroyed >= total && owningQuest.IsRunning() && !owningQuest.IsStageDone(StartFightStage)
		isStageDone = True
		UnregisterForHitEvent(Self)
		owningQuest.SetStage(StartFightStage)
	EndIf
EndFunction

Event OnAliasInit()
	; Crystals stay disarmed until the start stage restores them; a previous run can leave them destroyed.
	isStageDone = True
	crystalsDestroyed = 0
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
	If isStageDone
		Return
	EndIf
	If akTarget != None && !akTarget.IsDestroyed() && IsNonMeleeHit(akSource, akProjectile)
		; FO76 gates crystal damage by weapon type in DEST conditions, which FO4 cannot evaluate.
		akTarget.ClearDestruction()
		If akAggressor == Game.GetPlayer()
			ShowMeleeOnlyMessage()
		EndIf
	EndIf
	RegisterForHitEvent(Self)
EndEvent

Event OnDestructionStageChanged(ObjectReference akSenderRef, Int aiOldStage, Int aiCurrentStage)
	RefreshCrystalCount()
EndEvent

Event OnAliasShutdown()
	isStageDone = True
	UnregisterForHitEvent(Self)
EndEvent
