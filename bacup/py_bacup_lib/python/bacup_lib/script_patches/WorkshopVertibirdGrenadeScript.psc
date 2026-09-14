Event OnInit()
    If WorkshopVertibirdGrenadeKW == None || !WorkshopVertibirdGrenadeKW.SendStoryEventAndWait(GetCurrentLocation(), Self)
        If SQ_WorkshopVertibirdFailMessage != None
            SQ_WorkshopVertibirdFailMessage.Show()
        EndIf
    EndIf
EndEvent
