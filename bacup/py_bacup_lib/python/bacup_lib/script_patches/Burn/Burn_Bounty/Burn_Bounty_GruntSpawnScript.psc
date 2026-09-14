Function PrepareLocalTargets()
	If Act_BountyTarget == None || Act_BountyTarget.GetCount() == 0
		If !IsStageDone(9990)
			SetStage(9990)
		EndIf
		Return
	EndIf

	Int livingTargets = 0
	Int index = 0
	While index < Act_BountyTarget.GetCount()
		Actor target = Act_BountyTarget.GetAt(index) as Actor
		If target != None && !target.IsDead()
			target.Enable()
			target.EvaluatePackage()
			livingTargets += 1
		EndIf
		index += 1
	EndWhile
	If livingTargets == 0 && !IsStageDone(9990)
		SetStage(9990)
	EndIf
EndFunction

Function EngageLocalTargets()
	Actor targetPlayer = Alias_Player.GetActorReference()
	If targetPlayer == None
		targetPlayer = Game.GetPlayer()
	EndIf
	If targetPlayer == None || Act_BountyTarget == None
		Return
	EndIf

	Int index = 0
	While index < Act_BountyTarget.GetCount()
		Actor target = Act_BountyTarget.GetAt(index) as Actor
		If target != None && !target.IsDead()
			target.StartCombat(targetPlayer)
		EndIf
		index += 1
	EndWhile
EndFunction
