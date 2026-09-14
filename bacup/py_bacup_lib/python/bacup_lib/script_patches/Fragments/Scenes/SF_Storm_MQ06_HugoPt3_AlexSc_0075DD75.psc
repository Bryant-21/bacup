Function Fragment_Phase_05_Begin()
	ObjectReference furniture = Alias_Ref_AlexTrapBookFurniture.GetReference()
	Actor alex = Alias_Actor_Alex.GetActorReference()
	If furniture && alex
		furniture.Activate(alex)
	EndIf
EndFunction
