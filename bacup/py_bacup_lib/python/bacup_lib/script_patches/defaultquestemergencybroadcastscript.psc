Function SendEmergencyBroadcast(Topic akTopic, ReferenceAlias akSpeakerAlias)
    If akTopic == None
        Return
    EndIf

    Actor speaker
    If akSpeakerAlias != None
        speaker = akSpeakerAlias.GetActorReference()
    EndIf
    If speaker == None
        SQ_MasterScript masterScript = SQ_Master as SQ_MasterScript
        If masterScript != None
            speaker = masterScript.EBSDefaultActor
        EndIf
    EndIf
    If speaker == None
        Return
    EndIf
    If TurnOffActorValue != None && speaker.GetValue(TurnOffActorValue) == TurnOffAVValue
        Return
    EndIf

    speaker.Say(akTopic)
EndFunction

Event OnQuestInit()
    If TurnOffStage >= 0 && IsStageDone(TurnOffStage)
        Return
    EndIf

    SendEmergencyBroadcast(EBSTopic, EmergencyBroadcastActor)

    If EBSData != None
        Int index = 0
        While index < EBSData.Length
            If !EBSData[index].SendOnStageSet
                Topic broadcastTopic = EBSData[index].EBSTopic
                ReferenceAlias speakerAlias = EBSData[index].EmergencyBroadcastActor
                If broadcastTopic == None
                    broadcastTopic = EBSTopic
                EndIf
                If speakerAlias == None
                    speakerAlias = EmergencyBroadcastActor
                EndIf
                SendEmergencyBroadcast(broadcastTopic, speakerAlias)
            EndIf
            index += 1
        EndWhile
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If TurnOffStage >= 0 && IsStageDone(TurnOffStage)
        Return
    EndIf

    If EBSData != None
        Int index = 0
        While index < EBSData.Length
            If EBSData[index].SendOnStageSet && EBSData[index].StageOrMessageID == auiStageID
                Topic broadcastTopic = EBSData[index].EBSTopic
                ReferenceAlias speakerAlias = EBSData[index].EmergencyBroadcastActor
                If broadcastTopic == None
                    broadcastTopic = EBSTopic
                EndIf
                If speakerAlias == None
                    speakerAlias = EmergencyBroadcastActor
                EndIf
                SendEmergencyBroadcast(broadcastTopic, speakerAlias)
            EndIf
            index += 1
        EndWhile
    EndIf
EndEvent
