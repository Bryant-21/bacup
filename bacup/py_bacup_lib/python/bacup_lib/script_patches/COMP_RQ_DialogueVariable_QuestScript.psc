Event OnQuestInit()
	InitializeDialogueVariables()
EndEvent

Function InitializeDialogueVariables()
	Actor ally = Alias_Ally.GetActorReference()
	If !ally
		Return
	EndIf
	SetRandomValue(ally, COMP_RQ_FlavorVariable_Target, min_Target, max_Target)
	SetRandomValue(ally, COMP_RQ_FlavorVariable_QuestType, min_QuestType, max_QuestType)
	SetRandomValue(ally, COMP_RQ_FlavorVariable_Location, min_Location, max_Location)
	SetRandomValue(ally, COMP_RQ_FlavorVariable_IntelSource, min_IntelSource, max_IntelSource)
	SetRandomValue(ally, COMP_RQ_FlavorVariable_AcceptQuest, min_AcceptQuest, max_AcceptQuest)
	SetRandomValue(ally, COMP_RQ_FlavorVariable_Enemies, min_Enemies, max_Enemies)
EndFunction

Function SetRandomValue(Actor ally, ActorValue flavor, Int minimum, Int maximum)
	If !ally || !flavor
		Return
	EndIf
	If maximum < minimum
		maximum = minimum
	EndIf
	ally.SetValue(flavor, Utility.RandomInt(minimum, maximum))
EndFunction
