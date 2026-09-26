; Vinny's "[Start Event] I'm here to help guard the caravan" line. The INFO already requires
; E05_Caravan_Global_IsRunning == 1, which the dialogue quest's Master_QuestScript only opens while
; the activity is available, so the fragment just raises the story event: 560B13 is event scoped
; (ENAM = SCPT) and can only be started by E05_Caravan_QuestNode / E05_Caravan_EventNode matching
; the start keyword. E05_Caravan_Misc_StartKeyword is deliberately not sent: its quest is FO76's
; per-player "travel to Big Bend Tunnel" pointer, it has no Story Manager node in the conversion,
; and next to Vinny it would only add a misc objective that fails 120 seconds later.
Function Fragment_End(ObjectReference akSpeakerRef)
    If E05_Caravan_StartKeyword == None
        Return
    EndIf

    Actor player = Game.GetPlayer()
    If player == None
        Return
    EndIf

    Quest caravanQuest = Game.GetFormFromFile(0x00560B13, "SeventySix.esm") as Quest
    If caravanQuest != None && caravanQuest.IsRunning()
        Return
    EndIf

    E05_Caravan_StartKeyword.SendStoryEvent(player.GetCurrentLocation(), player, akSpeakerRef)
EndFunction
