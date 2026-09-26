Int Function PickerTimerID() Global
	Return 64312
EndFunction

Event OnAliasInit()
	B21DecidedActors = New Actor[0]
	B21CrateAttackers = New Actor[0]
	StartTimer(2.0, PickerTimerID())
EndEvent

Event OnAliasShutdown()
	CancelTimer(PickerTimerID())
	B21DecidedActors = New Actor[0]
	B21CrateAttackers = New Actor[0]
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != PickerTimerID()
		Return
	EndIf
	Quest owner = GetOwningQuest()
	If owner == None || !owner.IsRunning() || owner.IsStageDone(9000) || owner.IsStageDone(9998) || owner.IsStageDone(9999)
		Return
	EndIf
	PickTargetsForNewMembers()
	DamageCrate()
	StartTimer(2.0, PickerTimerID())
EndEvent

Function PickTargetsForNewMembers()
	If B21DecidedActors == None
		B21DecidedActors = New Actor[0]
	EndIf
	If B21CrateAttackers == None
		B21CrateAttackers = New Actor[0]
	EndIf
	Actor player = Game.GetPlayer()
	Int index = GetCount() - 1
	While index >= 0
		Actor member = GetAt(index) as Actor
		If member != None && !member.IsDead() && B21DecidedActors.Find(member) < 0
			B21DecidedActors.Add(member)
			If Utility.RandomInt(1, 100) <= TargetCrateChance
				B21CrateAttackers.Add(member)
				; The wave script opens combat on the player; crate attackers drop it so the alias travel package
				; (LinkCustom01 to the scrubber) can take them there.
				member.StopCombat()
				member.EvaluatePackage()
			ElseIf player != None
				member.StartCombat(player, True)
			EndIf
		EndIf
		index -= 1
	EndWhile
EndFunction

Function DamageCrate()
	If Crate == None || B21CrateAttackers == None
		Return
	EndIf
	ObjectReference crateRef = Crate.GetReference()
	If crateRef == None || !crateRef.Is3DLoaded() || crateRef.GetCurrentDestructionStage() > 0
		Return
	EndIf
	E08B_RadScrubberScript scrubberScript = Crate as E08B_RadScrubberScript
	If scrubberScript != None && scrubberScript.IgnoreDamageKeyword != None && crateRef.HasKeyword(scrubberScript.IgnoreDamageKeyword)
		Return
	EndIf

	Bool attacked = False
	Int index = B21CrateAttackers.Length - 1
	While index >= 0 && !attacked
		Actor attacker = B21CrateAttackers[index]
		If attacker == None || attacker.IsDead()
			B21CrateAttackers.Remove(index)
		ElseIf attacker.Is3DLoaded() && attacker.GetDistance(crateRef) <= 1024.0
			attacked = True
		EndIf
		index -= 1
	EndWhile
	If attacked
		; FO4 AI cannot target the scrubber, so attackers in reach wear it down. Its destructible data caps
		; damage at 2 DPS; both picker collections share that budget at 1 DPS each on the 2 s tick.
		crateRef.DamageObject(2.0)
	EndIf
EndFunction
