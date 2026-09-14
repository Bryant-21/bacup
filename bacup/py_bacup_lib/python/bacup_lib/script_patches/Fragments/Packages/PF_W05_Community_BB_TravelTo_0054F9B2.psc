Function Fragment_End(Actor akActor)
	If IsBoundCannibal(akActor)
		W05_Community_BB_Quest_Script controller = GetOwningQuest() as W05_Community_BB_Quest_Script
		If controller
			controller.CannibalReachedRoom(akActor)
		EndIf
	EndIf
EndFunction

Bool Function IsBoundCannibal(Actor akActor)
	ReferenceAlias firstCannibal = Cannibal01Alias as ReferenceAlias
	ReferenceAlias secondCannibal = Cannibal02Alias as ReferenceAlias
	ReferenceAlias thirdCannibal = Cannibal03Alias as ReferenceAlias
	Return akActor && ((firstCannibal && akActor == firstCannibal.GetActorReference()) || (secondCannibal && akActor == secondCannibal.GetActorReference()) || (thirdCannibal && akActor == thirdCannibal.GetActorReference()))
EndFunction
