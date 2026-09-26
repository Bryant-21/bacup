Function OnProgressItemsDeposited(Actor akActivator, Int aiCount)
	Quest radiationQuest = E05_Radiation
	If radiationQuest == None
		radiationQuest = GetOwningQuest()
	EndIf
	Quests:E05_Radiation:QuestScript eventScript = radiationQuest as Quests:E05_Radiation:QuestScript
	If eventScript != None
		eventScript.HandleOreDeposited(aiCount)
	EndIf
EndFunction
