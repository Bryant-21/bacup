Function EvaluateSwapCriteria()
	Bool criteriaMet = !ORCriteria
	Int i = 0
	While i < EnableStates.Length
		Bool thisMet = CriterionMet(EnableStates[i])
		If ORCriteria
			criteriaMet = criteriaMet || thisMet
		Else
			criteriaMet = criteriaMet && thisMet
		EndIf
		i += 1
	EndWhile

	If criteriaMet == EnableObject
		Enable()
	Else
		Disable()
	EndIf
EndFunction

Bool Function CriterionMet(EnableCriteria akCriteria)
	If akCriteria == None
		Return False
	EndIf
	Bool questMet = akCriteria.SwapQuest != None && akCriteria.SwapQuest.IsCompleted()
	If akCriteria.SwapAV == None
		Return questMet
	EndIf
	Bool avMet = ValueMeetsTarget(Game.GetPlayer().GetValue(akCriteria.SwapAV), akCriteria)
	If akCriteria.CheckBothQuestAndAV && akCriteria.SwapQuest != None
		Return questMet && avMet
	EndIf
	Return avMet
EndFunction

Bool Function ValueMeetsTarget(Float afValue, EnableCriteria akCriteria)
	If afValue == akCriteria.TargetValue
		Return True
	ElseIf afValue > akCriteria.TargetValue
		Return akCriteria.MaintainStateOnGreaterThanTargetValue
	EndIf
	Return akCriteria.MaintainStateOnLessThanTargetValue
EndFunction

Event OnInit()
	EvaluateSwapCriteria()
EndEvent

Event OnLoad()
	EvaluateSwapCriteria()
EndEvent
