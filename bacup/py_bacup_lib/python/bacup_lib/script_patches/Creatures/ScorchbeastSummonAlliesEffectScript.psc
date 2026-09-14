Event OnEffectStart(Actor akTarget, Actor akCaster)
	If IsAbilityEnabled && SQ_ScorchbeastSummonAlliesStart != None && akTarget != None
		SQ_ScorchbeastSummonAlliesStart.SendStoryEventAndWait(akTarget.GetCurrentLocation(), akTarget, akTarget)
	EndIf
EndEvent
