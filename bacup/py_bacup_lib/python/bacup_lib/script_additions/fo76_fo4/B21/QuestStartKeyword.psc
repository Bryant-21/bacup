Scriptname B21:QuestStartKeyword Extends Quest
{FO4 substitute for the FO76 quest start keyword.

FO76 quests name the keyword that offers them in QUST.QSSK, and the server's
daily content manager sent it. FO4 has neither the field nor the sender, so a
Story Manager node gated on that keyword can never fire — and a region daily
whose repeat node needs GetQuestCompleted == 1 can then never be reached at all,
because the initial node is the only way to earn that first completion. The
converter attaches this script carrying the keyword so a scheduler can send it.}

Keyword Property StartKeyword Auto Const
{The quest's FO76 QSSK keyword. Never None: the converter skips the binding
when the keyword did not survive conversion.}

Bool Function SendStartEvent(Location akLocation = None, ObjectReference akReference = None)
{Send this quest's own start event. True only when this quest is now running —
a conditionless sibling node can consume the event and start something else.}
	If StartKeyword == None
		Return False
	EndIf
	StartKeyword.SendStoryEventAndWait(akLocation, akReference)
	Return IsRunning()
EndFunction
