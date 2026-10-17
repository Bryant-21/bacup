Event OnAliasInit()
	OwningQuest = Self.GetOwningQuest()
	NumActorsInCombat = 0
EndEvent

Event OnCombatStateChanged(ObjectReference akSenderRef, Actor akTarget, Int aeCombatState)
	; FO76 announces once when the first member is attacked ("Protect the brahmin!") and
	; withdraws the objective once every member is out of combat again.
	If !OwningQuest
		OwningQuest = Self.GetOwningQuest()
	EndIf
	If aeCombatState == 1
		NumActorsInCombat += 1
		If NumActorsInCombat == 1
			If AnnounceMessage
				AnnounceMessage.Show()
			EndIf
			If ObjectiveToDisplay >= 0 && OwningQuest
				OwningQuest.SetObjectiveDisplayed(ObjectiveToDisplay, True)
			EndIf
		EndIf
	ElseIf aeCombatState == 0 && NumActorsInCombat > 0
		NumActorsInCombat -= 1
		If NumActorsInCombat == 0 && ObjectiveToDisplay >= 0 && OwningQuest
			OwningQuest.SetObjectiveDisplayed(ObjectiveToDisplay, False)
		EndIf
	EndIf
EndEvent
