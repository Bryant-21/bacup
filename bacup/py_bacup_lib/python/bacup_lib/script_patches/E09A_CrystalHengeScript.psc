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

CreatureUltraciteAbominationScript Function BossScript()
	If Alias_Boss == None
		Return None
	EndIf
	Return Alias_Boss as CreatureUltraciteAbominationScript
EndFunction

Function ResetHenge()
	hengesTotal = 0
	hengesDestroyed = 0
	UnregisterForHitEvent(Self)
	Int index = 0
	While index < GetCount()
		ObjectReference crystal = GetAt(index)
		If crystal != None
			crystal.ClearDestruction()
		EndIf
		index += 1
	EndWhile
EndFunction

Function ActivateHenge()
	hengesTotal = GetCount()
	hengesDestroyed = 0
	CreatureUltraciteAbominationScript boss = BossScript()
	If hengesTotal <= 0
		If boss != None
			boss.SetBossInvulnerable(False)
		EndIf
		Return
	EndIf
	RegisterForHitEvent(Self)
	RefreshHengeCount(True)
EndFunction

Function RefreshHengeCount(Bool abActivating)
	If hengesTotal <= 0
		Return
	EndIf
	Int destroyed = 0
	Int index = 0
	While index < GetCount()
		ObjectReference crystal = GetAt(index)
		If crystal != None && crystal.IsDestroyed()
			destroyed += 1
		EndIf
		index += 1
	EndWhile
	hengesDestroyed = destroyed

	CreatureUltraciteAbominationScript boss = BossScript()
	If destroyed >= hengesTotal
		; A zero total marks the henge as spent so late destruction events cannot toggle the boss again.
		hengesTotal = 0
		UnregisterForHitEvent(Self)
		If boss != None
			boss.SetBossInvulnerable(False)
		EndIf
	ElseIf abActivating && boss != None
		boss.SetBossInvulnerable(True)
	EndIf
EndFunction

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
	If hengesTotal <= 0
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
	RefreshHengeCount(False)
EndEvent

Event OnAliasShutdown()
	hengesTotal = 0
	UnregisterForHitEvent(Self)
EndEvent
