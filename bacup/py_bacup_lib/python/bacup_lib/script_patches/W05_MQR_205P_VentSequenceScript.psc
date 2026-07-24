; TODO

Event OnActivate(ObjectReference akActionRef)
    W05_MQR_205P_QuestScript owningQuestScript = GetOwningQuest() as W05_MQR_205P_QuestScript
    If owningQuestScript == None || VentButton == None || akActionRef != owningQuestScript.RaRa.GetReference()
        Return
    EndIf

    ObjectReference ventButtonRef = VentButton.GetReference()
    If ventButtonRef != None
        ventButtonRef.Activate(akActionRef)
    EndIf

    Scene ventScene = None
    Int currentStage = owningQuestScript.GetStage()
    If currentStage >= 1200
        ventScene = owningQuestScript.W05_MQR_205P_017_RaRa_LastVent02
    ElseIf currentStage >= 1100
        ventScene = owningQuestScript.W05_MQR_205P_016_RaRa_OptionalVent03
    ElseIf currentStage >= 900
        ventScene = owningQuestScript.W05_MQR_205P_014_RaRa_OverseerVent03
    EndIf
    If ventScene != None && !ventScene.IsPlaying()
        ventScene.Start()
    EndIf
EndEvent
