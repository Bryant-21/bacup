; FO76 ran this scheduler on the server. The FO4 substitute offers one Enclave
; event site to the Story Manager on the retained timer while no Enclave event
; runs and the player has joined the Enclave. SMQN 36398F then starts the event
; quest whose center marker type exists in the offered location.

Event OnQuestInit()
    bReadyForEvent = False
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    EN_ArmEventTimer(False)
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        EN_ArmEventTimer(True)
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(iEVTimerID)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != iEVTimerID || !IsRunning()
        Return
    EndIf
    If EN_ActiveEventQuest() != None || !EN_PlayerMayReceiveEvents()
        EN_ArmEventTimer(True)
        Return
    EndIf
    bReadyForEvent = True
    Location site = EN_PickEventSite()
    Bool started = False
    If site != None && EN_EventQuestStartKeyword != None
        started = EN_EventQuestStartKeyword.SendStoryEventAndWait(site)
    EndIf
    If started
        bReadyForEvent = False
    EndIf
    EN_ArmEventTimer(!started)
EndEvent

Function EN_ArmEventTimer(Bool abShort)
    Float delay = 0.0
    If abShort && EN_EventTimerLengthShort != None
        delay = EN_EventTimerLengthShort.GetValue()
    ElseIf EN_EventTimerLength != None
        delay = EN_EventTimerLength.GetValue()
    EndIf
    If delay < 1.0
        delay = 60.0
    EndIf
    CancelTimer(iEVTimerID)
    StartTimer(delay, iEVTimerID)
EndFunction

Quest Function EN_ActiveEventQuest()
    Int index = 0
    While EnclaveEventQuests != None && index < EnclaveEventQuests.Length
        Quest eventQuest = EnclaveEventQuests[index]
        If eventQuest != None && (eventQuest.IsRunning() || eventQuest.IsStarting() || eventQuest.IsStopping())
            Return eventQuest
        EndIf
        index += 1
    EndWhile
    Return None
EndFunction

Bool Function EN_PlayerMayReceiveEvents()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || playerRef.IsDead() || EnclaveEventQuests == None || EnclaveEventQuests.Length == 0
        Return False
    EndIf
    EnclaveEventQuestScript eventScript = EnclaveEventQuests[0] as EnclaveEventQuestScript
    If eventScript == None || eventScript.EN02_JoinedEnclaveValue == None
        Return False
    EndIf
    Return playerRef.GetValue(eventScript.EN02_JoinedEnclaveValue) > 0.0
EndFunction

; Source event centers: Bots POI01-05, the Dropped Connection towns and the
; A Real Blast sites. Harpers Ferry hosts both a Drop and a Blast center.
Location Function EN_PickEventSite()
    Int[] siteIDs = new Int[16]
    siteIDs[0] = 0x0012B7C4
    siteIDs[1] = 0x00254444
    siteIDs[2] = 0x00188B78
    siteIDs[3] = 0x0015E2CA
    siteIDs[4] = 0x00012F6B
    siteIDs[5] = 0x00006D9C
    siteIDs[6] = 0x0010CCEE
    siteIDs[7] = 0x0000414B
    siteIDs[8] = 0x00070368
    siteIDs[9] = 0x0006DE2D
    siteIDs[10] = 0x00004141
    siteIDs[11] = 0x0009A031
    siteIDs[12] = 0x0009A0D1
    siteIDs[13] = 0x00093D5F
    siteIDs[14] = 0x000B4871
    siteIDs[15] = 0x0005CC3C
    Int attempts = 0
    While attempts < siteIDs.Length
        Int pick = Utility.RandomInt(0, siteIDs.Length - 1)
        Location site = Game.GetFormFromFile(siteIDs[pick], "SeventySix.esm") as Location
        If site != None
            Return site
        EndIf
        attempts += 1
    EndWhile
    Return None
EndFunction
