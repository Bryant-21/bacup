Function WatchPlayerHits()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		RegisterForHitEvent(Self, playerRef)
	EndIf
EndFunction

Event OnAliasInit()
	myQI = GetOwningQuest()
	ArmedHitCount = 0
	WatchPlayerHits()
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
	If myQI == None
		myQI = GetOwningQuest()
	EndIf
	MTNS04QuestScript eventScript = myQI as MTNS04QuestScript
	If eventScript == None || !eventScript.IsRunning() || akAggressor != Game.GetPlayer()
		Return
	EndIf
	Weapon hitWeapon = akSource as Weapon
	Bool unarmed = hitWeapon == None || hitWeapon == UnarmedHuman || (UnarmedPowerArmor != None && hitWeapon == UnarmedPowerArmor)
	If !unarmed
		ArmedHitCount += 1
		If ArmedHitCount >= ArmedHitCountMax
			eventScript.bWendigoDefeatedUnarmed = False
			Return
		EndIf
	EndIf
	WatchPlayerHits()
EndEvent

Event OnAliasShutdown()
	UnregisterForAllHitEvents()
EndEvent
