Function UpdateLocalQuestState()
    If CharGenMasterQuest == None
        Return
    EndIf
    CharGenQuestStarted = CharGenMasterQuest.IsRunning() || CharGenMasterQuest.IsCompleted()
EndFunction

Event OnInit()
    UpdateLocalQuestState()
EndEvent

Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef as Actor == None || CharGenMasterQuest == None
        Return
    EndIf
    UpdateLocalQuestState()
    If !CharGenQuestStarted
        CharGenMasterQuest.Start()
        CharGenQuestStarted = True
    EndIf
EndEvent
